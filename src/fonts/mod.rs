pub(crate) mod catalog;
mod install;
mod paths;
mod uninstall;

use anyhow::Result;

use crate::cli::FontsCommand;

pub fn run(command: FontsCommand) -> Result<()> {
    match command {
        FontsCommand::List => list(),
        FontsCommand::Install { font, force } => install_cmd(&font, force),
        FontsCommand::Status { font } => status_cmd(&font),
        FontsCommand::Uninstall { font } => uninstall_cmd(&font),
    }
}

fn list() -> Result<()> {
    println!("{:<18} {:<14} DESCRIPTION", "FONT", "STATE");
    for font in catalog::CATALOG {
        let label = if paths::font_install_dir(font)?.exists() {
            "installed"
        } else {
            "not installed"
        };
        println!("{:<18} {:<14} {}", font.name, label, font.description);
    }
    Ok(())
}

fn status_cmd(font: &str) -> Result<()> {
    let spec = catalog::lookup_font(font)?;
    let dir = paths::font_install_dir(spec)?;
    println!("Font:              {}", spec.name);
    println!("Description:       {}", spec.description);
    println!("Path:              {}", dir.display());
    println!(
        "Exists:            {}",
        if dir.exists() { "yes" } else { "no" }
    );
    Ok(())
}

fn install_cmd(font: &str, force: bool) -> Result<()> {
    let spec = catalog::lookup_font(font)?;
    let dir = paths::font_install_dir(spec)?;
    if dir.exists() && !force {
        anyhow::bail!(
            "{} is already installed at {}; rerun with --force to reinstall",
            spec.name,
            dir.display()
        );
    }
    install::install_font(spec)?;
    println!("✓ Installed {} to {}", spec.name, dir.display());
    Ok(())
}

fn uninstall_cmd(font: &str) -> Result<()> {
    let spec = catalog::lookup_font(font)?;
    let dir = paths::font_install_dir(spec)?;
    if !dir.exists() {
        anyhow::bail!(
            "{} is not installed (expected at {})",
            spec.name,
            dir.display()
        );
    }
    uninstall::uninstall_font(spec)?;
    println!("✓ Uninstalled {}", spec.name);
    Ok(())
}
