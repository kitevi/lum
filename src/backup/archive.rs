use anyhow::{Context, Result, bail};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use std::fs::File;
use std::path::{Component, Path, PathBuf};
use tar::{Archive, Builder};

use super::target::{BackupTarget, GLOBAL_EXCLUDE_GLOBS};

const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

pub(crate) fn create_tar_gz(target: BackupTarget, home: &Path, output: &Path) -> Result<()> {
    let exclude_set = exclude_set(target)?;
    let target_path = target.target_path(home);
    let tar_gz =
        File::create(output).with_context(|| format!("failed to create {}", output.display()))?;
    let encoder = GzEncoder::new(tar_gz, Compression::default());
    let mut builder = Builder::new(encoder);

    let mut walker = WalkBuilder::new(&target_path);
    walker
        .hidden(false)
        .parents(false)
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false);

    for entry in walker.build() {
        let entry = entry.with_context(|| format!("failed to walk {}", target_path.display()))?;
        let path = entry.path();
        let archive_path = path
            .strip_prefix(home)
            .with_context(|| format!("failed to make archive path for {}", path.display()))?;

        if should_exclude(path, archive_path, &exclude_set) {
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                continue;
            }
            continue;
        }

        builder
            .append_path_with_name(path, archive_path)
            .with_context(|| format!("failed to add {} to archive", archive_path.display()))?;
    }

    let encoder = builder
        .into_inner()
        .context("failed to finish tar archive")?;
    encoder.finish().context("failed to finish gzip archive")?;
    Ok(())
}

/// Verify that every archive entry is inside the target directory.
pub(crate) fn verify_tar_gz(path: &Path, target: BackupTarget) -> Result<()> {
    verify_gzip_magic(path)?;
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    let target_root = Path::new(target.path);
    let mut found_target = false;

    for entry in archive.entries().context("failed to read tar entries")? {
        let entry = entry.context("failed to read tar entry")?;
        let entry_path = entry.path().context("failed to read tar entry path")?;
        // Ignore `./` prefixes from other tar implementations.
        let normalized: PathBuf = entry_path
            .components()
            .filter(|component| !matches!(component, Component::CurDir))
            .collect();
        if normalized
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            bail!(
                "archive entry {} contains '..'; refusing to restore",
                entry_path.display()
            );
        }
        if normalized.starts_with(target_root) {
            found_target = true;
        } else {
            bail!(
                "archive entry {} is outside the {} directory; refusing to restore",
                entry_path.display(),
                target.path
            );
        }
    }

    if !found_target {
        bail!("archive does not contain a {} directory", target.path);
    }

    Ok(())
}

/// Extract into staging on the home filesystem for a same-filesystem rename.
/// The caller moves the staged target and removes staging after success.
pub(crate) fn extract_to_staging(
    path: &Path,
    home: &Path,
    target: BackupTarget,
) -> Result<(PathBuf, PathBuf)> {
    let staging = tempfile::Builder::new()
        .prefix("lum-restore-")
        .tempdir_in(home)
        .context("failed to create restore staging directory")?;
    let staged_target = staging.path().join(target.path);
    extract_tar_gz(path, staging.path())?;
    require_staged_directory(&staged_target, target)?;
    Ok((staging.keep(), staged_target))
}

fn extract_tar_gz(path: &Path, destination: &Path) -> Result<()> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    archive
        .unpack(destination)
        .with_context(|| format!("failed to extract archive to {}", destination.display()))?;
    Ok(())
}

/// Only a real directory may replace the restored target, not a symlink.
fn require_staged_directory(staged_target: &Path, target: BackupTarget) -> Result<()> {
    let metadata = std::fs::symlink_metadata(staged_target)
        .with_context(|| format!("archive does not contain a {} directory", target.path))?;
    if !metadata.is_dir() {
        bail!(
            "archive {} entry is not a directory; refusing to restore",
            target.path
        );
    }
    Ok(())
}

fn verify_gzip_magic(path: &Path) -> Result<()> {
    use std::io::Read;

    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut magic = [0_u8; 2];
    file.read_exact(&mut magic)
        .context("file too small to be a valid tar.gz")?;
    if magic != GZIP_MAGIC {
        bail!("file is not a valid gzip archive (wrong magic bytes)");
    }
    Ok(())
}

fn exclude_set(target: BackupTarget) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in GLOBAL_EXCLUDE_GLOBS.iter().chain(target.excludes.iter()) {
        add_pattern(&mut builder, pattern)?;
    }
    builder.build().context("failed to build exclude glob set")
}

fn add_pattern(builder: &mut GlobSetBuilder, pattern: &str) -> Result<()> {
    add_glob(builder, pattern)?;
    add_glob(builder, &format!("{pattern}/**"))?;
    if !pattern.contains('/') {
        add_glob(builder, &format!("**/{pattern}"))?;
        add_glob(builder, &format!("**/{pattern}/**"))?;
    }
    Ok(())
}

fn add_glob(builder: &mut GlobSetBuilder, pattern: &str) -> Result<()> {
    builder.add(Glob::new(pattern).with_context(|| format!("invalid exclude glob {pattern}"))?);
    Ok(())
}

fn should_exclude(path: &Path, archive_path: &Path, exclude_set: &GlobSet) -> bool {
    exclude_set.is_match(archive_path)
        || path
            .file_name()
            .map(PathBuf::from)
            .is_some_and(|name| exclude_set.is_match(name))
}

#[cfg(test)]
mod tests {
    use super::super::target::BIO;

    use super::*;

    /// Build a tar.gz from ordered entries: (path, symlink target or None,
    /// file content). Symlink entries ignore their content.
    fn build_test_archive(path: &Path, entries: &[(&str, Option<&str>, &[u8])]) {
        let file = File::create(path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);
        for (name, link, content) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o644);
            if let Some(link) = link {
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_size(0);
                header.set_link_name(*link).unwrap();
            } else {
                header.set_size(content.len() as u64);
            }
            header.set_cksum();
            builder.append_data(&mut header, *name, *content).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
    }

    /// Bypass Builder's path validation to test raw paths containing `..`.
    fn build_raw_tar_gz(path: &Path, entries: &[(&str, &[u8])]) {
        let mut tar_bytes = Vec::new();
        for (name, content) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o644);
            header.set_uid(0);
            header.set_gid(0);
            header.set_size(content.len() as u64);
            header.set_entry_type(tar::EntryType::Regular);
            header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            tar_bytes.extend_from_slice(header.as_bytes());
            tar_bytes.extend_from_slice(content);
            let padding = (512 - content.len() % 512) % 512;
            tar_bytes.resize(tar_bytes.len() + padding, 0);
        }
        tar_bytes.extend_from_slice(&[0_u8; 1024]);
        let file = File::create(path).unwrap();
        let mut encoder = GzEncoder::new(file, Compression::default());
        std::io::Write::write_all(&mut encoder, &tar_bytes).unwrap();
        encoder.finish().unwrap();
    }

    #[test]
    fn verify_rejects_entries_outside_target() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("evil.tar.gz");
        build_test_archive(
            &archive,
            &[
                (".bio/keep.txt", None, b"data"),
                (".zshrc", None, b"pwned\n"),
                (".ssh/authorized_keys", None, b"ssh-ed25519 attacker\n"),
            ],
        );
        let error = verify_tar_gz(&archive, BIO).unwrap_err();
        assert!(error.to_string().contains("outside the .bio directory"));
    }

    #[test]
    fn verify_rejects_parent_directory_components() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("escape.tar.gz");
        build_raw_tar_gz(
            &archive,
            &[(".bio/keep.txt", b"data"), (".bio/../.zshrc", b"pwned\n")],
        );
        let error = verify_tar_gz(&archive, BIO).unwrap_err();
        assert!(error.to_string().contains("refusing to restore"));
    }

    #[test]
    fn verify_rejects_foreign_entries() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("other.tar.gz");
        build_raw_tar_gz(&archive, &[("unrelated/file.txt", b"x")]);
        let error = verify_tar_gz(&archive, BIO).unwrap_err();
        assert!(error.to_string().contains("outside the .bio directory"));
    }

    #[test]
    fn verify_rejects_empty_archive() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("empty.tar.gz");
        build_raw_tar_gz(&archive, &[]);
        let error = verify_tar_gz(&archive, BIO).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("does not contain a .bio directory")
        );
    }

    #[test]
    fn verify_accepts_own_backups_and_dot_slash_paths() {
        let home_dir = tempfile::tempdir().unwrap();
        let home = home_dir.path();
        std::fs::create_dir_all(home.join(".bio")).unwrap();
        std::fs::write(home.join(".bio/keep.txt"), "data").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("backup.tar.gz");
        create_tar_gz(BIO, home, &archive).unwrap();
        verify_tar_gz(&archive, BIO).unwrap();

        let normalized = dir.path().join("normalized.tar.gz");
        build_test_archive(&normalized, &[("./.bio/keep.txt", None, b"data")]);
        verify_tar_gz(&normalized, BIO).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn staged_extraction_confines_symlink_redirects() {
        let home_dir = tempfile::tempdir().unwrap();
        let home = home_dir.path();
        std::fs::write(home.join(".zshrc"), "legit\n").unwrap();
        std::fs::create_dir_all(home.join(".bio")).unwrap();
        std::fs::write(home.join(".bio/old.txt"), "old").unwrap();

        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("redirect.tar.gz");
        build_test_archive(
            &archive,
            &[
                (".bio/keep.txt", None, b"data"),
                // Redirect a later write outside the target directory.
                (".bio/esc", Some(".."), b""),
                (".bio/esc/evil.txt", None, b"pwned\n"),
            ],
        );
        verify_tar_gz(&archive, BIO).unwrap();

        let (staging_dir, staged_target) = extract_to_staging(&archive, home, BIO).unwrap();
        std::fs::rename(home.join(".bio"), home.join(".bio.old")).unwrap();
        std::fs::rename(&staged_target, home.join(".bio")).unwrap();
        std::fs::remove_dir_all(&staging_dir).unwrap();

        assert_eq!(
            std::fs::read_to_string(home.join(".zshrc")).unwrap(),
            "legit\n"
        );
        assert_eq!(
            std::fs::read_to_string(home.join(".bio/keep.txt")).unwrap(),
            "data"
        );
        assert!(!home.join("evil.txt").exists());
    }

    #[test]
    #[cfg(unix)]
    fn staged_extraction_rejects_symlinked_target_root() {
        let home_dir = tempfile::tempdir().unwrap();
        let home = home_dir.path();
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("root-symlink.tar.gz");
        build_test_archive(
            &archive,
            &[
                (".bio", Some("../elsewhere"), b""),
                (".bio/keep.txt", None, b"data"),
            ],
        );
        // Valid entry paths can still produce a symlinked target root.
        verify_tar_gz(&archive, BIO).unwrap();
        assert!(extract_to_staging(&archive, home, BIO).is_err());
        assert!(std::fs::read_dir(home).unwrap().next().is_none());
    }
}
