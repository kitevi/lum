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
        MirrorCommand::Sync { jobs } => sync(jobs.0),
        MirrorCommand::Status { jobs, offline } => status(jobs.0, offline),
        MirrorCommand::Watch { tag, cycles } => watch::run(&tag, cycles),
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

/// The configured repos, or `None` once it has said that there are none.
fn configured_repos() -> Result<Option<Vec<config::RepoEntry>>> {
    let path = config::config_path()?;
    if !path.exists() {
        println!("{}", not_configured(&path));
        return Ok(None);
    }
    let repos = config::load(&path)?;
    if repos.is_empty() {
        println!("No repos configured");
        return Ok(None);
    }
    Ok(Some(repos))
}

/// What to say when the config file does not exist yet.
fn not_configured(path: &std::path::Path) -> String {
    format!(
        "No repos configured (config file not found: {}). Run `lum repos mirror init` to create it.",
        path.display()
    )
}

fn list() -> Result<()> {
    let Some(repos) = configured_repos()? else {
        return Ok(());
    };
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

fn sync(jobs: u64) -> Result<()> {
    ensure_git_on_path()?;
    let Some(repos) = configured_repos()? else {
        return Ok(());
    };
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

/// Reports each mirror as up to date, behind or missing. What it finds is output, not an
/// exit code: only a mirror it could not check fails the command.
fn status(jobs: u64, offline: bool) -> Result<()> {
    ensure_git_on_path()?;
    let Some(repos) = configured_repos()? else {
        return Ok(());
    };
    let mirror = mirror_dir()?;
    let any_error = super::worker_pool(jobs, repos.len())?.install(|| {
        repos
            .par_iter()
            .map(|repo| status_one(repo, &mirror, offline))
            .reduce(|| false, |any_error, failed| any_error || failed)
    });
    if any_error {
        anyhow::bail!("some mirrors could not be checked");
    }
    Ok(())
}

/// Prints how one mirror compares with its remote; true when it could not check it.
fn status_one(repo: &config::RepoEntry, mirror: &std::path::Path, offline: bool) -> bool {
    let dir_name = repo.directory_name();
    let dst = mirror.join(&dir_name);

    if !dst.exists() {
        println!("{}: missing", dst.display());
        return false;
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
                    true
                }
            },
            Err(e) => {
                eprintln!("error reading local HEAD for {}: {e}", dst.display());
                true
            }
        }
    } else {
        match git::git_remote_head(&repo.url, &repo.branch) {
            Ok(remote) => match git::git_local_head(&dst) {
                Ok(local) => status_from_heads(&dst, local, remote),
                Err(e) => {
                    eprintln!("error reading local HEAD for {}: {e}", dst.display());
                    true
                }
            },
            Err(e) => {
                eprintln!("error checking remote {}: {e}", dir_name);
                true
            }
        }
    }
}

/// Prints whether a mirror is up to date; false, since checking it worked.
fn status_from_heads(dst: &std::path::Path, local: String, remote: String) -> bool {
    let state = if local == remote {
        "up to date"
    } else {
        "behind"
    };
    println!("{}: {state}", dst.display());
    false
}
