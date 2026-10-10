mod catalog;
mod install;
mod platform;
mod state;
mod version;

use std::path::Path;

use anyhow::Result;

use crate::cli::ToolsCommand;

pub fn run(command: ToolsCommand) -> Result<()> {
    match command {
        ToolsCommand::List => list(),
        ToolsCommand::Install { tool, force } => install_cmd(&tool, force),
        ToolsCommand::Status { tool } => status_cmd(&tool),
        ToolsCommand::Sync { dry_run } => sync_cmd(dry_run),
        ToolsCommand::Update { tool, force } => update_cmd(&tool, force),
        ToolsCommand::Version { tool } => version_cmd(&tool),
    }
}

fn list() -> Result<()> {
    println!(
        "{:<18} {:<12} {:<12} DESCRIPTION",
        "TOOL", "BINARY", "STATE"
    );
    for tool in catalog::CATALOG {
        let status = state::local_status(tool)?;
        let label = if status.managed && status.exists {
            "installed"
        } else if status.managed {
            "missing"
        } else if status.exists {
            "unmanaged"
        } else {
            "available"
        };
        println!(
            "{:<18} {:<12} {:<12} {}",
            tool.name, tool.binary, label, tool.description
        );
    }
    Ok(())
}

fn status_cmd(tool: &str) -> Result<()> {
    let spec = catalog::lookup_tool(tool)?;
    let status = state::local_status(spec)?;
    println!("Tool:              {}", spec.name);
    println!("Binary:            {}", spec.binary);
    println!("Managed:           {}", yes_no(status.managed));
    println!("Path:              {}", status.path.display());
    println!("Exists:            {}", yes_no(status.exists));
    println!(
        "Installed version: {}",
        status.effective_version().unwrap_or("unknown")
    );
    Ok(())
}

fn install_cmd(tool: &str, force: bool) -> Result<()> {
    let spec = catalog::lookup_tool(tool)?;
    let status = state::local_status(spec)?;
    if status.exists && !status.managed && !force {
        anyhow::bail!(
            "{} already exists at {} but is not managed by lum; rerun with --force to overwrite it",
            spec.name,
            status.path.display()
        );
    }
    if status.exists && status.managed && !force {
        anyhow::bail!(
            "{} is already installed at {}; use 'lum tools update {}' or rerun with --force",
            spec.name,
            status.path.display(),
            spec.name
        );
    }
    let artifact = resolve_latest(spec)?;
    let installed = install::install_artifact(spec, &artifact, None)?;
    println!(
        "✓ Installed {} {} to {}",
        spec.name,
        installed.installed_version,
        installed.path.display()
    );
    Ok(())
}

fn update_cmd(tool: &str, force: bool) -> Result<()> {
    let spec = catalog::lookup_tool(tool)?;
    let status = state::local_status(spec)?;
    if !status.managed {
        anyhow::bail!(
            "{} is not installed; use 'lum tools install {}' first",
            spec.name,
            spec.name
        );
    }
    let artifact = resolve_latest(spec)?;
    let previous = status.effective_version().unwrap_or("unknown").to_owned();
    if !force
        && previous != "unknown"
        && version::compare_versions(spec, &previous, &artifact.version)?.is_ge()
    {
        println!("{} is already up to date ({})", spec.name, previous);
        return Ok(());
    }
    let updated = install::install_artifact(spec, &artifact, status.installed_at()?)?;
    println!(
        "✓ Updated {} {} -> {} at {}",
        spec.name,
        previous,
        updated.installed_version,
        updated.path.display()
    );
    Ok(())
}

fn sync_cmd(dry_run: bool) -> Result<()> {
    let mut installed = 0;
    let mut updated = 0;
    let mut up_to_date = 0;
    for spec in catalog::CATALOG {
        let status = state::local_status(spec)?;
        let artifact = resolve_latest(spec)?;
        match status.effective_version() {
            None if dry_run => println!("• {}: would install {}", spec.name, artifact.version),
            None => {
                print!("• {}: installing {}... ", spec.name, artifact.version);
                std::io::Write::flush(&mut std::io::stdout())?;
                install::install_artifact(spec, &artifact, None)?;
                println!("done");
                installed += 1;
            }
            Some(current)
                if version::compare_versions(spec, current, &artifact.version)?.is_lt()
                    && dry_run =>
            {
                println!(
                    "• {}: would update {} -> {}",
                    spec.name, current, artifact.version
                )
            }
            Some(current)
                if version::compare_versions(spec, current, &artifact.version)?.is_lt() =>
            {
                print!(
                    "• {}: updating {} -> {}... ",
                    spec.name, current, artifact.version
                );
                std::io::Write::flush(&mut std::io::stdout())?;
                install::install_artifact(spec, &artifact, status.installed_at()?)?;
                println!("done");
                updated += 1;
            }
            Some(current) => {
                println!("• {}: up to date ({})", spec.name, current);
                up_to_date += 1;
            }
        }
    }
    println!("\nSummary: {installed} installed, {updated} updated, {up_to_date} up to date");
    Ok(())
}

fn version_cmd(tool: &str) -> Result<()> {
    let spec = catalog::lookup_tool(tool)?;
    let status = state::local_status(spec)?;
    let artifact = resolve_latest(spec)?;
    println!(
        "Installed: {}",
        status.effective_version().unwrap_or("unknown")
    );
    println!("Latest:    {}", artifact.version);
    Ok(())
}

fn resolve_latest(spec: &catalog::ToolSpec) -> Result<platform::Artifact> {
    if let Ok(path) = std::env::var(catalog::test_artifact_env(spec)) {
        return Ok(platform::Artifact {
            version: "test".into(),
            release_tag: "test".into(),
            asset_name: Path::new(&path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            download_url: path,
            archive_type: platform::ArchiveType::Binary,
            binary_path: platform::installed_filename(spec.binary),
            checksum_sha256: None,
        });
    }
    let release = crate::github::latest_release(spec.owner, spec.repo)?;
    platform::artifact_for_release(spec, &release)
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
