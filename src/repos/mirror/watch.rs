use anyhow::Result;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use crate::paths;
use crate::repos::mirror::config;
use crate::repos::mirror::git;

/// A detected change in a remote branch's HEAD.
pub struct Change {
    pub basename: String,
    pub branch: String,
    pub old_sha: Option<String>,
    pub new_sha: String,
}

pub fn run(tag: &str, cycles: Option<usize>) -> Result<()> {
    let config_path = config::config_path()?;
    anyhow::ensure!(
        config_path.exists(),
        "{}",
        super::not_configured(&config_path)
    );
    let repos = config::load(&config_path)?;
    let matching: Vec<_> = repos.iter().filter(|r| r.has_tag(tag)).collect();
    if matching.is_empty() {
        let tags: BTreeSet<&str> = repos
            .iter()
            .flat_map(|repo| repo.tags.iter().map(String::as_str))
            .collect();
        return Err(crate::cli::unknown_name("tag", tag, tags));
    }

    println!("{}", format_startup_banner(tag, &matching));
    send_startup_notification(tag);
    let state_path = paths::repos_mirror_watch_state_file()?;
    let cycle_count = cycles.unwrap_or(usize::MAX);

    for _ in 0..cycle_count {
        let changes = check_cycle(&matching, &state_path)?;

        for change in &changes {
            let short_sha = &change.new_sha[..7];
            let old_info = change
                .old_sha
                .as_ref()
                .map(|s| s[..7].to_string())
                .unwrap_or_default();
            println!(
                "{}/{} — HEAD changed to {} (was {})",
                change.basename, change.branch, short_sha, old_info
            );
            send_notification(&change.basename, &change.branch, short_sha);
        }

        if cycle_count == usize::MAX {
            std::thread::sleep(std::time::Duration::from_secs(300));
        }
    }

    Ok(())
}

/// Run a single poll cycle. Returns changes detected.
pub fn check_cycle(repos: &[&config::RepoEntry], state_path: &Path) -> Result<Vec<Change>> {
    let mut state = load_state(state_path)?;
    let mut changes = Vec::new();

    for repo in repos {
        let key = format!("{} {}", repo.url, repo.branch);
        let basename = url_basename(&repo.url);

        match git::git_remote_head(&repo.url, &repo.branch) {
            Ok(new_sha) => {
                if let Some(old_sha) = state.get(&key)
                    && old_sha != &new_sha
                {
                    changes.push(Change {
                        basename,
                        branch: repo.branch.clone(),
                        old_sha: Some(old_sha.clone()),
                        new_sha: new_sha.clone(),
                    });
                }
                state.insert(key, new_sha);
            }
            Err(e) => {
                eprintln!("warning: failed to check {}/{}: {e}", basename, repo.branch);
            }
        }
    }

    save_state(state_path, &state)?;
    Ok(changes)
}

fn url_basename(url: &str) -> String {
    let url = url.strip_prefix("file://").unwrap_or(url);
    Path::new(url)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| url.to_string())
        .trim_end_matches(".git")
        .to_string()
}

/// Format the startup banner for the watch command.
pub fn format_startup_banner(tag: &str, repos: &[&config::RepoEntry]) -> String {
    let mut lines = vec![startup_notification_body(tag)];
    for repo in repos {
        lines.push(format!("  {}/{}", url_basename(&repo.url), repo.branch));
    }
    lines.join("\n")
}

/// Format the startup notification body.
pub fn startup_notification_body(tag: &str) -> String {
    format!("Watching repositories with the {tag} tag")
}

/// Send a startup notification indicating watch has begun.
fn send_startup_notification(tag: &str) {
    if std::env::var("LUM_NO_NOTIFY").is_ok() {
        return;
    }
    let summary = "lum repos mirror watch".to_string();
    let body = startup_notification_body(tag);
    if let Err(e) = notify_rust::Notification::new()
        .summary(&summary)
        .body(&body)
        .show()
    {
        eprintln!("warning: failed to send desktop notification: {e}");
    }
}

fn load_state(path: &Path) -> Result<HashMap<String, String>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = std::fs::read_to_string(path)?;
    let state: HashMap<String, String> = serde_json::from_str(&content)?;
    Ok(state)
}

fn save_state(path: &Path, state: &HashMap<String, String>) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state)?;
    std::fs::write(path, json)?;
    Ok(())
}

fn send_notification(basename: &str, branch: &str, short_sha: &str) {
    if std::env::var("LUM_NO_NOTIFY").is_ok() {
        return;
    }
    let summary = format!("{basename}/{branch}");
    let body = format!("HEAD changed to {short_sha}");
    if let Err(e) = notify_rust::Notification::new()
        .summary(&summary)
        .body(&body)
        .show()
    {
        eprintln!("warning: failed to send desktop notification: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_message_with_matching_tag() {
        let repos = [
            config::RepoEntry {
                url: "https://github.com/example/repo-a.git".into(),
                branch: "main".into(),
                tags: vec!["metrocargo".into()],
            },
            config::RepoEntry {
                url: "https://github.com/example/repo-b.git".into(),
                branch: "develop".into(),
                tags: vec!["metrocargo".into(), "other".into()],
            },
        ];
        let matching: Vec<_> = repos.iter().filter(|r| r.has_tag("metrocargo")).collect();
        let banner = format_startup_banner("metrocargo", &matching);
        assert_eq!(
            banner,
            "Watching repositories with the metrocargo tag\n  repo-a/main\n  repo-b/develop"
        );
    }

    #[test]
    fn startup_banner_with_no_repos() {
        let repos: Vec<config::RepoEntry> = vec![];
        let matching: Vec<_> = repos.iter().filter(|r| r.has_tag("nothing")).collect();
        let banner = format_startup_banner("nothing", &matching);
        assert_eq!(banner, "Watching repositories with the nothing tag");
    }

    #[test]
    fn startup_notification_body_contains_tag() {
        let body = startup_notification_body("metrocargo");
        assert_eq!(body, "Watching repositories with the metrocargo tag");
    }
}
