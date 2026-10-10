use anyhow::{Context, Result};
use chrono::Local;
use std::path::Path;

use super::archive;
use super::target::{BackupTarget, KEEP_BACKUPS};
use super::upload;

pub(crate) async fn restore_backup(target: BackupTarget, code: &str) -> Result<()> {
    println!(
        "{} - Starting {} restore from code: {}",
        timestamp(),
        target.name,
        code
    );
    let home = crate::paths::home_dir()?;
    let client = upload::client()?;
    let url = upload::restore_url(code);
    let tmp = tempfile::Builder::new()
        .prefix(&format!("{}-restore-", target.name))
        .suffix(".tar.gz")
        .tempfile()
        .context("failed to create temporary file")?;
    let tmp_path = tmp.path().to_owned();

    println!("{} - Downloading...", timestamp());
    upload::download_archive(&client, &url, &tmp_path).await?;

    println!("{} - Verifying downloaded archive...", timestamp());
    let verify_path = tmp_path.clone();
    tokio::task::spawn_blocking(move || archive::verify_tar_gz(&verify_path, target))
        .await
        .context("archive verification task failed")?
        .context(
            "downloaded file is not a valid tar.gz archive; you may have entered the wrong code or the file may have expired",
        )?;
    let size_mb = tmp.as_file().metadata()?.len() as f64 / (1024.0 * 1024.0);
    println!(
        "{} - Archive verified (size: {:.2} MB)",
        timestamp(),
        size_mb
    );

    println!(
        "{} - Extracting archive to a staging directory...",
        timestamp()
    );
    let extract_path = tmp_path.clone();
    let extract_home = home.clone();
    let (staging_dir, staged_target) = tokio::task::spawn_blocking(move || {
        archive::extract_to_staging(&extract_path, &extract_home, target)
    })
    .await
    .context("archive extraction task failed")?
    .context("failed to extract archive")?;

    let target_path = target.target_path(&home);
    let mut existing_backup = None;
    if target_path.exists() {
        println!(
            "{} - Existing {} directory found, creating backup...",
            timestamp(),
            target.name
        );
        let backup_path = home.join(format!(
            "{}{}",
            target.backup_prefix,
            Local::now().format("%Y%m%d-%H%M%S")
        ));
        std::fs::rename(&target_path, &backup_path).with_context(|| {
            format!(
                "failed to backup existing {} to {}",
                target.name,
                backup_path.display()
            )
        })?;
        println!(
            "{} - Backup created at {}",
            timestamp(),
            backup_path.display()
        );
        existing_backup = Some(backup_path);
        cleanup_old_backups(&home, target.backup_prefix, KEEP_BACKUPS)?;
    }

    println!("{} - Moving {} into place...", timestamp(), target.name);
    let moved = std::fs::rename(&staged_target, &target_path);
    let cleaned = std::fs::remove_dir_all(&staging_dir);
    if let Err(error) = moved {
        if let Some(backup_path) = &existing_backup {
            println!("{} - Move failed, restoring backup...", timestamp());
            let _ = std::fs::rename(backup_path, &target_path);
        }
        return Err(error).context("failed to move restored directory into place");
    }
    if let Err(error) = cleaned {
        println!(
            "{} - Warning: failed to remove staging directory: {error}",
            timestamp()
        );
    }

    println!("{} - {} restored successfully!", timestamp(), target.name);
    if let Some(backup_path) = existing_backup {
        println!(
            "{} - Previous {} backed up to: {}",
            timestamp(),
            target.name,
            backup_path.display()
        );
    }
    println!("{} - Temporary archive removed.", timestamp());
    Ok(())
}

fn cleanup_old_backups(home: &Path, backup_prefix: &str, keep_count: usize) -> Result<()> {
    let mut backups = Vec::new();
    for entry in
        std::fs::read_dir(home).with_context(|| format!("failed to read {}", home.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(backup_prefix) {
            continue;
        }
        let metadata = entry.metadata()?;
        backups.push((entry.path(), metadata.modified()?));
    }

    backups.sort_by(|(_, a), (_, b)| b.cmp(a));
    for (path, _) in backups.into_iter().skip(keep_count) {
        match std::fs::remove_dir_all(&path) {
            Ok(()) => println!(
                "{} - Removed old backup: {}",
                timestamp(),
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("<unknown>")
            ),
            Err(error) => println!(
                "{} - Warning: failed to remove old backup {}: {}",
                timestamp(),
                path.display(),
                error
            ),
        }
    }
    Ok(())
}

fn timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M").to_string()
}
