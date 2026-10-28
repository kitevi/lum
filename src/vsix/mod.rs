mod catalog;
mod editor;
mod release;
mod requirements;

use std::collections::BTreeMap;

use anyhow::Result;

use crate::cli::{VsixCommand, VsixEditor};
use catalog::VsixSpec;
use editor::Editor;

pub fn run(command: VsixCommand) -> Result<()> {
    match command {
        VsixCommand::List { editor } => list(editor),
        VsixCommand::Install {
            extension,
            editor,
            force,
        } => install_cmd(&extension, editor, force),
        VsixCommand::Status { extension, editor } => status_cmd(&extension, editor),
        VsixCommand::Update {
            extension,
            editor,
            force,
        } => update_cmd(&extension, editor, force),
        VsixCommand::Sync { editor, dry_run } => sync_cmd(editor, dry_run),
        VsixCommand::Version { extension, editor } => version_cmd(&extension, editor),
    }
}

fn install_cmd(name: &str, choice: Option<VsixEditor>, force: bool) -> Result<()> {
    let spec = catalog::lookup_extension(name)?;
    requirements::ensure(spec)?;
    let editor = editor::resolve(choice)?;
    let installed = editor.installed_extensions()?;
    if !force && let Some(current) = editor::installed_version(&installed, spec.extension_id) {
        anyhow::bail!(
            "{} {current} is already installed in {}; use 'lum vsix update {}' or rerun with --force",
            spec.name,
            editor.name,
            spec.name
        );
    }
    let artifact = release::resolve_latest(spec)?;
    let installed = release::install(spec, &artifact, &editor)?;
    println!(
        "\u{2713} Installed {} {} into {}",
        spec.name, artifact.version, editor.name
    );
    conflict_notes(spec, &editor, &installed);
    Ok(())
}

fn update_cmd(name: &str, choice: Option<VsixEditor>, force: bool) -> Result<()> {
    let spec = catalog::lookup_extension(name)?;
    requirements::ensure(spec)?;
    let editor = editor::resolve(choice)?;
    let installed = editor.installed_extensions()?;
    let Some(previous) = editor::installed_version(&installed, spec.extension_id) else {
        anyhow::bail!(
            "{} is not installed in {}; use 'lum vsix install {}' first",
            spec.name,
            editor.name,
            spec.name
        );
    };
    let artifact = release::resolve_latest(spec)?;
    if !force && previous == artifact.version {
        println!("{} is already up to date ({previous})", spec.name);
        return Ok(());
    }
    let previous = previous.to_owned();
    let installed = release::install(spec, &artifact, &editor)?;
    println!(
        "\u{2713} Updated {} {previous} -> {} in {}",
        spec.name, artifact.version, editor.name
    );
    conflict_notes(spec, &editor, &installed);
    Ok(())
}

fn sync_cmd(choice: Option<VsixEditor>, dry_run: bool) -> Result<()> {
    let editor = editor::resolve(choice)?;
    let mut installed = editor.installed_extensions()?;
    let mut fresh = 0;
    let mut updated = 0;
    let mut up_to_date = 0;
    let mut skipped = 0;
    for spec in catalog::CATALOG {
        if let Some(reason) = requirements::missing(spec) {
            println!("\u{2022} {}: skipped; {reason}", spec.name);
            skipped += 1;
            continue;
        }
        let artifact = release::resolve_latest(spec)?;
        match editor::installed_version(&installed, spec.extension_id).map(str::to_owned) {
            None if dry_run => {
                println!("\u{2022} {}: would install {}", spec.name, artifact.version)
            }
            None => {
                print!(
                    "\u{2022} {}: installing {}... ",
                    spec.name, artifact.version
                );
                std::io::Write::flush(&mut std::io::stdout())?;
                installed = release::install(spec, &artifact, &editor)?;
                println!("done");
                conflict_notes(spec, &editor, &installed);
                fresh += 1;
            }
            Some(current) if current != artifact.version && dry_run => {
                println!(
                    "\u{2022} {}: would update {current} -> {}",
                    spec.name, artifact.version
                )
            }
            Some(current) if current != artifact.version => {
                print!(
                    "\u{2022} {}: updating {current} -> {}... ",
                    spec.name, artifact.version
                );
                std::io::Write::flush(&mut std::io::stdout())?;
                installed = release::install(spec, &artifact, &editor)?;
                println!("done");
                conflict_notes(spec, &editor, &installed);
                updated += 1;
            }
            Some(current) => {
                println!("\u{2022} {}: up to date ({current})", spec.name);
                up_to_date += 1;
            }
        }
    }
    println!(
        "\nSummary: {fresh} installed, {updated} updated, {up_to_date} up to date, {skipped} skipped ({})",
        editor.name
    );
    Ok(())
}

fn version_cmd(name: &str, choice: Option<VsixEditor>) -> Result<()> {
    let spec = catalog::lookup_extension(name)?;
    let editor = editor::resolve(choice)?;
    let installed = editor.installed_extensions()?;
    let artifact = release::resolve_latest(spec)?;
    println!(
        "Installed: {}",
        editor::installed_version(&installed, spec.extension_id).unwrap_or("not installed")
    );
    println!("Latest:    {}", artifact.version);
    Ok(())
}

fn status_cmd(name: &str, choice: Option<VsixEditor>) -> Result<()> {
    let spec = catalog::lookup_extension(name)?;
    println!("Extension:         {}", spec.name);
    println!("ID:                {}", spec.extension_id);
    println!("Source:            github.com/{}/{}", spec.owner, spec.repo);
    for requirement in spec.requires {
        let found = requirement
            .locate()
            .map_or_else(|| "missing".to_owned(), |path| path.display().to_string());
        println!("{:<19}{found}", format!("{}:", requirement.label()));
    }
    let editor = match editor::resolve(choice) {
        Ok(editor) => editor,
        Err(error) => {
            println!("Editor:            {error}");
            return Ok(());
        }
    };
    let installed = editor.installed_extensions()?;
    println!(
        "Editor:            {} ({})",
        editor.name,
        editor.path.display()
    );
    println!(
        "Installed version: {}",
        editor::installed_version(&installed, spec.extension_id).unwrap_or("not installed")
    );
    let conflicts = installed_conflicts(spec, &installed);
    if !conflicts.is_empty() {
        println!("Conflicts:         {}", conflicts.join(", "));
    }
    Ok(())
}

fn list(choice: Option<VsixEditor>) -> Result<()> {
    let installed = match editor::resolve(choice) {
        Ok(editor) => {
            println!("Editor: {} ({})\n", editor.name, editor.path.display());
            Some(editor.installed_extensions()?)
        }
        Err(error) => {
            println!("Editor: {error}\n");
            None
        }
    };
    println!(
        "{:<12} {:<16} {:<14} {:<14} DESCRIPTION",
        "EXTENSION", "ID", "INSTALLED", "REQUIRES"
    );
    for spec in catalog::CATALOG {
        let state = match &installed {
            Some(installed) => editor::installed_version(installed, spec.extension_id)
                .unwrap_or("not installed")
                .to_owned(),
            None => "unknown".to_owned(),
        };
        let requires = spec
            .requires
            .iter()
            .map(|requirement| {
                if requirement.locate().is_some() {
                    requirement.label()
                } else {
                    format!("{} (missing)", requirement.label())
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "{:<12} {:<16} {:<14} {:<14} {}",
            spec.name, spec.extension_id, state, requires, spec.description
        );
    }
    Ok(())
}

fn installed_conflicts(spec: &VsixSpec, installed: &BTreeMap<String, String>) -> Vec<&'static str> {
    spec.conflicts
        .iter()
        .copied()
        .filter(|id| editor::installed_version(installed, id).is_some())
        .collect()
}

fn conflict_notes(spec: &VsixSpec, editor: &Editor, installed: &BTreeMap<String, String>) {
    for id in installed_conflicts(spec, installed) {
        println!(
            "Note: {id} is also installed in {}; disable it in workspaces that use {}, or both will attach to the same files.",
            editor.name, spec.name
        );
    }
}
