use anyhow::{Context, Result};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use super::config::{Identity, data_dir};
use crate::paths::home_path;

pub fn read_optional(path: &Path) -> Result<String> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.into()),
    }
}

pub fn replace_marked_section(path: &Path, begin: &str, end: &str, section: &str) -> Result<()> {
    let existing = read_optional(path)?;
    let stripped = strip_marked_section(&existing, begin, end)
        .with_context(|| format!("updating {}", path.display()))?;
    std::fs::write(path, append_managed_section(&stripped, section))?;
    Ok(())
}

/// Remove a marked section, leaving a file without one untouched. Returns what was removed.
pub fn remove_marked_section(path: &Path, begin: &str, end: &str) -> Result<Option<String>> {
    let existing = read_optional(path)?;
    let stripped = strip_marked_section(&existing, begin, end)
        .with_context(|| format!("updating {}", path.display()))?;
    if stripped == existing {
        return Ok(None);
    }
    let remaining = stripped.trim_end();
    let content = if remaining.is_empty() {
        String::new()
    } else {
        format!("{remaining}\n")
    };
    std::fs::write(path, content)?;
    let removed: Vec<_> = existing
        .lines()
        .skip_while(|line| line.trim() != begin)
        .skip(1)
        .take_while(|line| line.trim() != end)
        .collect();
    Ok(Some(removed.join("\n")))
}

/// Whether settings follow the marked section, where they override anything in it.
pub fn has_settings_after_section(content: &str, end: &str) -> bool {
    content.lines().any(|line| line.trim() == end)
        && content
            .lines()
            .rev()
            .map(str::trim)
            .take_while(|line| *line != end)
            .any(|line| !(line.is_empty() || line.starts_with('#') || line.starts_with(';')))
}

pub fn is_lum_managed_file(path: &Path, marker: &str) -> Result<bool> {
    Ok(read_optional(path)?.starts_with(marker))
}

fn strip_marked_section(content: &str, begin: &str, end: &str) -> Result<String> {
    let mut output = String::with_capacity(content.len());
    let mut in_section = false;
    for line in content.split_inclusive('\n') {
        let marker = line.trim();
        if marker == begin {
            if in_section {
                anyhow::bail!("found {begin} twice without {end}; fix the file by hand");
            }
            in_section = true;
        } else if marker == end {
            if !in_section {
                anyhow::bail!("found {end} without {begin}; fix the file by hand");
            }
            in_section = false;
        } else if !in_section {
            output.push_str(line);
        }
    }
    if in_section {
        anyhow::bail!("found {begin} without {end}; fix the file by hand");
    }
    Ok(output)
}

fn append_managed_section(existing_without_section: &str, section: &str) -> String {
    let existing = existing_without_section.trim_end();
    if existing.is_empty() {
        section.to_string()
    } else {
        format!("{existing}\n\n{section}")
    }
}

pub fn cleanup_old_backups() -> Result<()> {
    let backup_dir = data_dir()?.join("backups");
    if !backup_dir.exists() {
        return Ok(());
    }
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 24 * 60 * 60);
    for entry in std::fs::read_dir(backup_dir)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.modified().unwrap_or(std::time::SystemTime::now()) < cutoff {
            let _ = std::fs::remove_file(entry.path());
        } else if metadata.is_file() {
            // Older versions created archives readable by other users.
            crate::private_file::harden(&entry.path())?;
        }
    }
    Ok(())
}

/// What an identity no longer in the config left behind: the lum-marked files `sync` backs up
/// and removes, and `status` reports.
#[derive(Default)]
pub struct Orphan {
    pub name: String,
    git_config: Option<PathBuf>,
    key_pair: Vec<PathBuf>,
}

impl Orphan {
    /// The files, in words: `key pair and git config`, `key pair` or `git config`.
    pub fn contents(&self) -> &'static str {
        match (self.key_pair.is_empty(), self.git_config.is_some()) {
            (false, true) => "key pair and git config",
            (false, false) => "key pair",
            (true, _) => "git config",
        }
    }

    fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.git_config.iter().chain(&self.key_pair)
    }
}

/// The identities missing from `identities` whose lum-marked Git config or key pair is still
/// there, by name. An unmarked file is never an orphan.
pub fn find_orphans(identities: &[Identity]) -> Result<Vec<Orphan>> {
    let active: HashSet<&str> = identities
        .iter()
        .map(|identity| identity.name.as_str())
        .collect();
    let mut orphans = BTreeMap::<String, Orphan>::new();
    for entry in std::fs::read_dir(crate::paths::home_dir()?)? {
        let path = entry?.path();
        let Some(name) =
            file_name(&path).and_then(|name| name.strip_prefix(".gitconfig-lum-git-id-"))
        else {
            continue;
        };
        if !active.contains(name)
            && is_lum_managed_file(&path, &format!("# lum:git-id:managed identity={name}"))?
        {
            orphans.entry(name.to_owned()).or_default().git_config = Some(path.clone());
        }
    }
    let ssh_dir = home_path(".ssh")?;
    if ssh_dir.exists() {
        for entry in std::fs::read_dir(&ssh_dir)? {
            let public_key = entry?.path();
            let Some(name) = file_name(&public_key)
                .and_then(|name| name.strip_prefix("lum-git-id-"))
                .and_then(|name| name.strip_suffix(".pub"))
            else {
                continue;
            };
            if active.contains(name)
                || !read_optional(&public_key)?.contains(&format!("[lum:git-id identity={name}]"))
            {
                continue;
            }
            let private_key = ssh_dir.join(format!("lum-git-id-{name}"));
            let key_pair = &mut orphans.entry(name.to_owned()).or_default().key_pair;
            key_pair.push(public_key.clone());
            if private_key.exists() {
                key_pair.push(private_key);
            }
        }
    }
    Ok(orphans
        .into_iter()
        .map(|(name, orphan)| Orphan { name, ..orphan })
        .collect())
}

fn file_name(path: &Path) -> Option<&str> {
    path.file_name()?.to_str()
}

/// Archives an orphan's files where only the owner can read them, then removes them. Returns
/// the archive.
pub fn back_up_and_remove(orphan: &Orphan) -> Result<PathBuf> {
    let backup_dir = data_dir()?.join("backups");
    std::fs::create_dir_all(&backup_dir)?;
    // Archives can hold unencrypted private keys: tempfile names are unique and owner-only.
    let archive = tempfile::Builder::new()
        .prefix("git-id-orphans-")
        .suffix(".tar.gz")
        .tempfile_in(&backup_dir)?;
    let encoder = flate2::write::GzEncoder::new(archive.as_file(), flate2::Compression::default());
    let mut tar = tar::Builder::new(encoder);
    for path in orphan.paths() {
        tar.append_path_with_name(path, path.file_name().unwrap_or_default())?;
    }
    tar.into_inner()?.finish()?;
    archive.as_file().sync_all()?;
    let (_, archive) = archive.keep()?;
    for path in orphan.paths() {
        std::fs::remove_file(path)?;
    }
    Ok(archive)
}
