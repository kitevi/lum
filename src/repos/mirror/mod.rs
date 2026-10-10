use anyhow::Result;
use rayon::prelude::*;

use super::ensure_git_on_path;
use crate::cli::MirrorCommand;

pub mod config;
mod git;
mod watch;

pub fn run(command: MirrorCommand) -> Result<()> {
    match command {
        MirrorCommand::ConfigPath => {
            let path = config::config_path()?;
            println!("{}", path.display());
            Ok(())
        }
        MirrorCommand::Dir => {
            let dir = mirror_dir()?;
            println!("{}", dir.display());
            Ok(())
        }
        MirrorCommand::Init => init(),
        MirrorCommand::List => list(),
        MirrorCommand::Sync { jobs } => sync(jobs),
        MirrorCommand::Status { jobs, offline } => status(jobs, offline),
        MirrorCommand::Watch { tag, cycles } => watch::run(tag, cycles),
    }
}
fn mirror_dir() -> Result<std::path::PathBuf> {
    // Try XDG_DOCUMENTS_DIR first (for testability), then fall back to
    // the directories crate's resolution.
    let docs = if let Ok(xdg) = std::env::var("XDG_DOCUMENTS_DIR") {
        std::path::PathBuf::from(xdg)
    } else {
        let user_dirs = directories::UserDirs::new()
            .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
        user_dirs
            .document_dir()
            .ok_or_else(|| anyhow::anyhow!("cannot determine Documents directory"))?
            .to_path_buf()
    };
    Ok(docs.join("CodeMirror"))
}

fn init() -> Result<()> {
    let path = config::config_path()?;
    if path.exists() {
        println!("{} already exists, not overwriting", path.display());
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let sample = serde_json::json!({
        "repos": [{
            "url": "https://github.com/example-org/example-repo.git",
            "branch": "main",
            "tags": ["sample"]
        }]
    });
    let json = serde_json::to_string_pretty(&sample)?;
    std::fs::write(&path, json)?;
    println!("Created {} with a sample repository entry", path.display());
    Ok(())
}

fn list() -> Result<()> {
    let path = config::config_path()?;
    if !path.exists() {
        println!(
            "No repos configured (config file not found: {})",
            path.display()
        );
        return Ok(());
    }
    let repos = config::load(&path)?;
    if repos.is_empty() {
        println!("No repos configured");
        return Ok(());
    }
    for repo in &repos {
        let tags = if repo.tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", repo.tags.join(", "))
        };
        println!(
            "{}  {}  {}{}",
            repo.directory_name(),
            repo.url,
            repo.branch,
            tags
        );
    }
    Ok(())
}

fn sync(jobs: usize) -> Result<()> {
    ensure_git_on_path()?;
    let path = config::config_path()?;
    if !path.exists() {
        println!(
            "No repos configured (config file not found: {})",
            path.display()
        );
        return Ok(());
    }
    let repos = config::load(&path)?;
    if repos.is_empty() {
        println!("No repos configured");
        return Ok(());
    }
    let mirror = mirror_dir()?;
    // Ensure mirror directory exists
    std::fs::create_dir_all(&mirror)
        .map_err(|e| anyhow::anyhow!("creating mirror directory: {e}"))?;

    let any_error = super::worker_pool(jobs, repos.len())?.install(|| {
        repos
            .par_iter()
            .map(|repo| sync_one(repo, &mirror))
            .reduce(|| false, |any_error, failed| any_error || failed)
    });
    if any_error {
        anyhow::bail!("some mirror operations failed");
    }
    Ok(())
}

fn sync_one(repo: &config::RepoEntry, mirror: &std::path::Path) -> bool {
    let dir_name = repo.directory_name();
    let dst = mirror.join(&dir_name);
    if dst.exists() {
        println!("  {} (updating)", dir_name);
        if let Err(e) = git::git_update_shallow(&dst, &repo.branch, mirror) {
            eprintln!("error updating {}: {e}", dir_name);
            return true;
        }
    } else {
        println!("+ {} (cloning)", dir_name);
        if let Err(e) = git::git_clone_shallow(&repo.url, &repo.branch, &dst, mirror) {
            eprintln!("error cloning {}: {e}", dir_name);
            return true;
        }
    }
    false
}

fn status(jobs: usize, offline: bool) -> Result<()> {
    ensure_git_on_path()?;
    let path = config::config_path()?;
    if !path.exists() {
        println!(
            "No repos configured (config file not found: {})",
            path.display()
        );
        return Ok(());
    }
    let repos = config::load(&path)?;
    if repos.is_empty() {
        println!("No repos configured");
        return Ok(());
    }
    let mirror = mirror_dir()?;
    let outcomes = super::worker_pool(jobs, repos.len())?.install(|| {
        repos
            .par_iter()
            .map(|repo| status_one(repo, &mirror, offline))
            .collect::<Vec<_>>()
    });
    let mut any_behind = false;
    let mut any_error = false;
    for outcome in outcomes {
        any_behind |= outcome.behind;
        any_error |= outcome.error;
    }

    if any_error {
        std::process::exit(1);
    }
    if any_behind {
        std::process::exit(2);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct StatusOutcome {
    behind: bool,
    error: bool,
}

fn status_one(repo: &config::RepoEntry, mirror: &std::path::Path, offline: bool) -> StatusOutcome {
    let dir_name = repo.directory_name();
    let dst = mirror.join(&dir_name);

    if !dst.exists() {
        println!("{}: missing", dst.display());
        return StatusOutcome {
            behind: true,
            error: false,
        };
    }

    if offline {
        match git::git_local_head(&dst) {
            Ok(local) => match git::git_local_origin_head(&dst, &repo.branch) {
                Ok(remote) => status_from_heads(&dst, local, remote),
                Err(e) => {
                    eprintln!(
                        "error reading origin/{} for {}: {e}",
                        repo.branch,
                        dst.display()
                    );
                    StatusOutcome {
                        behind: false,
                        error: true,
                    }
                }
            },
            Err(e) => {
                eprintln!("error reading local HEAD for {}: {e}", dst.display());
                StatusOutcome {
                    behind: false,
                    error: true,
                }
            }
        }
    } else {
        match git::git_remote_head(&repo.url, &repo.branch) {
            Ok(remote) => match git::git_local_head(&dst) {
                Ok(local) => status_from_heads(&dst, local, remote),
                Err(e) => {
                    eprintln!("error reading local HEAD for {}: {e}", dst.display());
                    StatusOutcome {
                        behind: false,
                        error: true,
                    }
                }
            },
            Err(e) => {
                eprintln!("error checking remote {}: {e}", dir_name);
                StatusOutcome {
                    behind: false,
                    error: true,
                }
            }
        }
    }
}

fn status_from_heads(dst: &std::path::Path, local: String, remote: String) -> StatusOutcome {
    if local == remote {
        println!("{}: up to date", dst.display());
        StatusOutcome {
            behind: false,
            error: false,
        }
    } else {
        println!("{}: behind", dst.display());
        StatusOutcome {
            behind: true,
            error: false,
        }
    }
}
