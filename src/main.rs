
use std::collections::HashSet;
use std::fs::File;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use git2::{Repository, ObjectType};
use log::debug;
use serde::{Serialize, Deserialize};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long)]
    repo: String,
    #[arg(long)]
    old_rev: String,
    #[arg(long)]
    new_rev: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ImageConfig {
    image:     String,
    tag:       String,
    build_num: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BuildSpec {
    image: String,
    tag:   String,
    path:  PathBuf,

    build_num: Option<String>,
}

fn main() -> Result<()> {
    env_logger::init();
    let args = Args::try_parse()?;
    let repo = Repository::open(&args.repo)?;
    let repo_path = Path::new(&args.repo);

    let r1 = repo
        .revparse_single(&args.old_rev)
        .context("failed to find old revision")?;
    debug!("old_rev = {}", r1.id());

    let r2 = repo
        .revparse_single(&args.new_rev)
        .context("failed to find new revision")?;
    debug!("new_rev = {}", r2.id());
    
    let t1 = r1.peel(ObjectType::Tree)?;
    let t2 = r2.peel(ObjectType::Tree)?;

    let diff = repo.diff_tree_to_tree(t1.as_tree(), t2.as_tree(), None)?;

    let specs = diff.deltas()
        .map(|d| {
            let new_path = match d.new_file().path() {
                Some(p) => p,
                None => return Ok(None),
            };
            let f_path   = repo_path.join(new_path);
            let f_parent = f_path.parent().context("unexpected missing parent")?;
            let d_full   = f_parent.canonicalize().context("failed to canonicalize path")?;
            debug!("identified modified path: {}", d_full.display());
            Ok(Some(d_full))
        })
        .collect::<Result<Vec<Option<PathBuf>>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<PathBuf>>()
        .into_iter()
        .collect::<HashSet<PathBuf>>()
        .into_iter()
        .map(|p| {
            let meta_path = p.join(Path::new("image.yaml"));
            debug!("checking path: {}", meta_path.display());
            let meta_file = match File::open(meta_path) {
                Ok(f) => f,
                Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e).context("failed to open file"),
            };
            let config: ImageConfig = serde_yaml::from_reader(meta_file)?;
            let spec = BuildSpec {
                image: config.image,
                tag:   config.tag,
                path:  p,
                build_num: config.build_num,
            };
            Ok(Some(spec))
        })
        .collect::<Result<Vec<Option<BuildSpec>>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<BuildSpec>>();

    let out = std::io::stdout();
    serde_json::to_writer(out, &specs)?;
    Ok(())
}
