use anyhow::{Context, Result};
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

pub fn cleanup_orphan_identity_configs<F>(
    identities: &[Identity],
    identity_git_config_path: F,
) -> Result<()>
where
    F: Fn(&Identity) -> Result<PathBuf>,
{
    let home = crate::paths::home_dir()?;
    let active: std::collections::HashSet<_> = identities
        .iter()
        .map(|identity| identity.name.clone())
        .collect();
    for entry in std::fs::read_dir(&home)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(identity_name) = name.strip_prefix(".gitconfig-lum-git-id-")
            && !active.contains(identity_name)
            && is_lum_managed_file(
                &entry.path(),
                &format!("# lum:git-id:managed identity={identity_name}"),
            )?
        {
            backup_and_remove(&[entry.path()])?;
        }
    }
    let _ = identity_git_config_path;
    Ok(())
}

pub fn cleanup_orphan_key_pairs(identities: &[Identity]) -> Result<()> {
    let ssh_dir = home_path(".ssh")?;
    if !ssh_dir.exists() {
        return Ok(());
    }
    let active: std::collections::HashSet<_> = identities
        .iter()
        .map(|identity| identity.name.clone())
        .collect();
    for entry in std::fs::read_dir(&ssh_dir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(identity_name) = file_name
            .strip_prefix("lum-git-id-")
            .and_then(|name| name.strip_suffix(".pub"))
        else {
            continue;
        };
        if active.contains(identity_name) {
            continue;
        }
        let marker = format!("[lum:git-id identity={identity_name}]");
        let content = read_optional(&path)?;
        if !content.contains(&marker) {
            continue;
        }
        let private_key = ssh_dir.join(format!("lum-git-id-{identity_name}"));
        let mut paths = vec![path];
        if private_key.exists() {
            paths.push(private_key);
        }
        backup_and_remove(&paths)?;
    }
    Ok(())
}

fn backup_and_remove(paths: &[PathBuf]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let backup_dir = data_dir()?.join("backups");
    std::fs::create_dir_all(&backup_dir)?;
    // Archives can hold unencrypted private keys: tempfile names are unique and owner-only.
    let archive = tempfile::Builder::new()
        .prefix("git-id-orphans-")
        .suffix(".tar.gz")
        .tempfile_in(&backup_dir)?;
    let encoder = flate2::write::GzEncoder::new(archive.as_file(), flate2::Compression::default());
    let mut tar = tar::Builder::new(encoder);
    for path in paths {
        if path.exists() {
            tar.append_path_with_name(path, path.file_name().unwrap_or_default())?;
        }
    }
    tar.into_inner()?.finish()?;
    archive.as_file().sync_all()?;
    archive.keep()?;
    for path in paths {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
