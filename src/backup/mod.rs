use anyhow::{Context, Result};
mod archive;
mod pre_command;
mod restore;
mod target;
mod upload;

use crate::cli::{BackupCommand, BackupTargetName};

pub fn run(command: BackupCommand) -> Result<()> {
    match command {
        BackupCommand::Create { target } => upload_target(allowed(target)?),
        BackupCommand::Restore { target, code } => restore::restore_backup(allowed(target)?, &code),
    }
}

/// The target a name stands for, once this OS is one it is backed up on.
fn allowed(name: BackupTargetName) -> Result<target::BackupTarget> {
    let target = match name {
        BackupTargetName::Bio => target::BIO,
        BackupTargetName::Roms => target::ROMS,
    };
    target.ensure_current_os_allowed()?;
    Ok(target)
}

fn upload_target(target: target::BackupTarget) -> Result<()> {
    let home = crate::paths::home_dir()?;
    let target_path = target.target_path(&home);
    if !target_path.exists() {
        anyhow::bail!(
            "{} directory not found at {}",
            target.name,
            target_path.display()
        );
    }

    println!("{} - Starting {} backup", timestamp(), target.name);
    println!(
        "This will archive your ~/{} and upload it for backup/sharing",
        target.path
    );
    println!();
    println!(
        "{} - Found {} directory at {}",
        timestamp(),
        target.name,
        target_path.display()
    );

    if let Some(program) = target.pre_command
        && let Some(path) = pre_command::find(program)
    {
        println!("{} - Running {program}...", timestamp());
        pre_command::run(&path)?;
    }

    let archive_file = tempfile::Builder::new()
        .prefix(&format!("{}-backup-", target.name))
        .suffix(".tar.gz")
        .tempfile()?;
    let archive_path = archive_file.path().to_owned();

    println!("{} - Creating compressed archive...", timestamp());
    archive::create_tar_gz(target, &home, &archive_path)?;
    let size_mb = archive_file.as_file().metadata()?.len() as f64 / (1024.0 * 1024.0);
    println!(
        "{} - Archive created successfully (size: {:.2} MB)",
        timestamp(),
        size_mb
    );

    println!("{} - Uploading...", timestamp());
    let url = upload::upload_archive(&archive_path)?;

    println!("{} - Verifying upload...", timestamp());
    let verify_file = tempfile::Builder::new()
        .prefix(&format!("{}-verify-", target.name))
        .suffix(".tar.gz")
        .tempfile()?;
    crate::github::download_asset(&url, verify_file.path())?;
    archive::verify_tar_gz(verify_file.path(), target).context(
        "upload verification failed; received file may be an error page instead of archive",
    )?;

    println!("{} - Upload verified successfully!", timestamp());
    println!(
        "{} - Your {} backup is available at:",
        timestamp(),
        target.name
    );
    println!("{url}");
    let code = upload::code_from_url(&url)?;
    println!(
        "{} - Restore with: lum backup restore {} {}",
        timestamp(),
        target.name,
        code
    );
    println!("{} - Temporary archive removed.", timestamp());
    Ok(())
}

fn timestamp() -> String {
    jiff::Zoned::now().strftime("%Y-%m-%d %H:%M").to_string()
}
