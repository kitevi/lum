pub(crate) mod catalog;
mod install;
mod launcher;
mod package;
mod settings;
mod state;

use std::{
    io::{self, Write as _},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};

use crate::cli::AppsCommand;
use catalog::{AppSpec, Build};
use settings::SettingsStatus;

pub fn run(command: AppsCommand) -> Result<()> {
    match command {
        AppsCommand::List => list(),
        AppsCommand::Install { app, force } => install_cmd(&app, force),
        AppsCommand::Status { app } => status_cmd(&app),
        AppsCommand::Update { app, force } => update_cmd(&app, force),
        AppsCommand::Sync { dry_run } => sync_cmd(dry_run),
        AppsCommand::Uninstall { app } => uninstall_cmd(&app),
    }
}

fn list() -> Result<()> {
    println!(
        "{:<10} {:<14} {:<10} {:<12} DESCRIPTION",
        "APP", "STATE", "VERSION", "SETTINGS"
    );
    for spec in catalog::apps() {
        let (state, version, settings) = match spec.build() {
            None => ("unavailable", None, None),
            Some(build) => {
                let status = state::local_status(spec, build)?;
                let state = match (status.managed, status.exists) {
                    (true, true) => "installed",
                    (false, true) => "unmanaged",
                    _ => "not installed",
                };
                let settings = settings_status(spec, build)?.map(|status| describe(&status));
                (
                    state,
                    status.installed_version().map(str::to_owned),
                    settings,
                )
            }
        };
        let settings = settings.unwrap_or_else(|| "-".into());
        println!(
            "{:<10} {:<14} {:<10} {:<12} {}",
            spec.name,
            state,
            version.as_deref().unwrap_or("-"),
            settings,
            spec.description
        );
    }
    for spec in catalog::apps() {
        if let Some(build) = spec.build() {
            for path in state::foreign_copies(spec, build)? {
                println!("foreign: {} ({})", path.display(), spec.name);
            }
        }
    }
    Ok(())
}

fn status_cmd(app: &str) -> Result<()> {
    let spec = catalog::lookup_app(app)?;
    field("App:", spec.name);
    match spec.build() {
        None => field("Available:", &format!("no build for {}", platform())),
        Some(build) => {
            let status = state::local_status(spec, build)?;
            field("Managed:", yes_no(status.managed));
            field("Path:", &status.path.display().to_string());
            field("Exists:", yes_no(status.exists));
            field(
                "Installed version:",
                status.stored_version.as_deref().unwrap_or("unknown"),
            );
            let latest = match install::resolve_latest(spec, build) {
                Ok(artifact) => artifact.version,
                Err(error) => format!("unknown ({error:#})"),
            };
            field("Latest version:", &latest);
            if let Some(launcher) = launcher::path(spec, &build.package)? {
                field("Launcher:", &launcher.display().to_string());
            }
            let foreign = state::foreign_copies(spec, build)?;
            if !foreign.is_empty() {
                field("Foreign copies:", &join_paths(&foreign));
            }
            if let Some(status) = settings_status(spec, build)? {
                field("Settings file:", &status.file.display().to_string());
                field("Settings:", &describe(&status));
                print_drift(&status, "  ");
            }
        }
    }
    Ok(())
}

fn install_cmd(app: &str, force: bool) -> Result<()> {
    let spec = catalog::lookup_app(app)?;
    let build = spec.require_build()?;
    ensure_no_foreign_copies(spec, build)?;
    let status = state::local_status(spec, build)?;
    if status.exists && !status.managed && !force {
        bail!(
            "{} already exists at {} but is not managed by lum; rerun with --force to replace it",
            spec.name,
            status.path.display()
        );
    }
    if status.exists && status.managed && !force {
        bail!(
            "{} is already installed at {}; use `lum apps update {}` or rerun with --force",
            spec.name,
            status.path.display(),
            spec.name
        );
    }
    ensure_not_running(spec, build)?;
    let artifact = install::resolve_latest(spec, build)?;
    let installed = install::install_app(spec, build, &artifact)?;
    println!(
        "\u{2713} Installed {} {} to {}",
        spec.name,
        installed.installed_version,
        installed.path.display()
    );
    print_note(build);
    apply_settings(spec, build)
}

fn update_cmd(app: &str, force: bool) -> Result<()> {
    let spec = catalog::lookup_app(app)?;
    let build = spec.require_build()?;
    let status = state::local_status(spec, build)?;
    if !status.managed {
        bail!(
            "{} is not installed; use `lum apps install {}` first",
            spec.name,
            spec.name
        );
    }
    ensure_no_foreign_copies(spec, build)?;
    let artifact = install::resolve_latest(spec, build)?;
    let previous = status.installed_version().unwrap_or("unknown");
    if !force && previous == artifact.version {
        println!("{} is already up to date ({previous})", spec.name);
    } else {
        ensure_not_running(spec, build)?;
        let updated = install::install_app(spec, build, &artifact)?;
        println!(
            "\u{2713} Updated {} {previous} -> {} at {}",
            spec.name,
            updated.installed_version,
            updated.path.display()
        );
    }
    apply_settings(spec, build)
}

fn sync_cmd(dry_run: bool) -> Result<()> {
    let (mut installed, mut updated, mut up_to_date, mut configured, mut skipped) = (0, 0, 0, 0, 0);
    for spec in catalog::apps() {
        let Some(build) = spec.build() else {
            println!("\u{2022} {}: no build for {}", spec.name, platform());
            skipped += 1;
            continue;
        };
        let status = state::local_status(spec, build)?;
        if status.exists && !status.managed {
            println!(
                "\u{2022} {}: unmanaged at {}; rerun `lum apps install {} --force` to take over",
                spec.name,
                status.path.display(),
                spec.name
            );
            skipped += 1;
            continue;
        }
        ensure_no_foreign_copies(spec, build)?;
        let artifact = install::resolve_latest(spec, build)?;
        let latest = artifact.version.as_str();
        let current = status.installed_version();
        if current == Some(latest) {
            println!("\u{2022} {}: up to date ({latest})", spec.name);
            up_to_date += 1;
        } else {
            let (doing, would) = match current {
                Some(current) => (
                    format!("updating {current} -> {latest}"),
                    format!("would update {current} -> {latest}"),
                ),
                None => (
                    format!("installing {latest}"),
                    format!("would install {latest}"),
                ),
            };
            if dry_run {
                println!("\u{2022} {}: {would}", spec.name);
            } else {
                ensure_not_running(spec, build)?;
                print!("\u{2022} {}: {doing}... ", spec.name);
                io::stdout().flush()?;
                install::install_app(spec, build, &artifact)?;
                println!("done");
                if current.is_some() {
                    updated += 1;
                } else {
                    print_note(build);
                    installed += 1;
                }
            }
        }
        let Some(settings) = settings_status(spec, build)? else {
            continue;
        };
        if settings.drift.is_empty() {
            continue;
        }
        let changes = count(settings.drift.len(), "setting");
        if dry_run {
            println!(
                "\u{2022} {}: would set {changes} in {}",
                spec.name,
                settings.file.display()
            );
            print_drift(&settings, "    ");
        } else {
            ensure_not_running(spec, build)?;
            settings::apply(&settings)?;
            println!(
                "\u{2022} {}: set {changes} in {}",
                spec.name,
                settings.file.display()
            );
            configured += 1;
        }
    }
    println!(
        "\nSummary: {installed} installed, {updated} updated, {up_to_date} up to date, {configured} configured, {skipped} skipped"
    );
    Ok(())
}

fn uninstall_cmd(app: &str) -> Result<()> {
    let spec = catalog::lookup_app(app)?;
    let build = spec.require_build()?;
    let status = state::local_status(spec, build)?;
    if !status.managed {
        bail!("{} is not installed by lum", spec.name);
    }
    ensure_not_running(spec, build)?;
    let settings_file = build
        .settings
        .map(|settings| (settings.file)(&status.path))
        .transpose()?;
    install::uninstall(spec, build)?;
    println!(
        "\u{2713} Uninstalled {} from {}",
        spec.name,
        status.path.display()
    );
    match settings_file {
        Some(file) if file.starts_with(&status.path) => {
            println!("  Its settings were in that folder and went with it");
        }
        Some(file) => println!("  Its settings stay in {}", file.display()),
        None => {}
    }
    Ok(())
}

/// The owned settings of a build, read where the app is installed.
fn settings_status(spec: &AppSpec, build: &Build) -> Result<Option<SettingsStatus>> {
    let Some(settings) = build.settings else {
        return Ok(None);
    };
    Ok(Some(settings::status(
        settings,
        &state::app_path(spec, build)?,
    )?))
}

/// Bring the app's owned settings to lum's values.
fn apply_settings(spec: &AppSpec, build: &Build) -> Result<()> {
    let Some(status) = settings_status(spec, build)? else {
        return Ok(());
    };
    if status.drift.is_empty() {
        return Ok(());
    }
    ensure_not_running(spec, build)?;
    settings::apply(&status)?;
    println!(
        "\u{2713} Set {} in {}",
        count(status.drift.len(), "setting"),
        status.file.display()
    );
    Ok(())
}

/// lum leaves a running app alone: mGBA rewrites its config on every settings
/// change and ROM load, and Windows cannot replace a running program's files.
fn ensure_not_running(spec: &AppSpec, build: &Build) -> Result<()> {
    let name = build.process_name(spec);
    let mut system = sysinfo::System::new().context("failed to list running processes")?;
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    // Linux lists a process under the name it was started by, cut to 15
    // characters: an AppImage started through its AppRun symlink is "AppRun".
    // The executable's own file name is the reliable match.
    let running = system.processes().values().any(|process| {
        process.name().eq_ignore_ascii_case(&name)
            || process
                .exe()
                .and_then(Path::file_name)
                .is_some_and(|file| file.eq_ignore_ascii_case(&name))
    });
    if running {
        bail!("{} is running; quit it and try again", spec.name);
    }
    Ok(())
}

fn ensure_no_foreign_copies(spec: &AppSpec, build: &Build) -> Result<()> {
    let foreign = state::foreign_copies(spec, build)?;
    if foreign.is_empty() {
        return Ok(());
    }
    bail!(
        "{} exists at {} and is not managed by lum; remove it first — lum never installs alongside a foreign copy",
        spec.name,
        join_paths(&foreign)
    );
}

/// Run a system tool quietly, failing with its error output.
fn run_tool(command: &mut Command) -> Result<()> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to run {program}"))?;
    if !output.status.success() {
        bail!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn remove_file_if_present(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            Err(error).with_context(|| format!("failed to remove {}", path.display()))
        }
        _ => Ok(()),
    }
}

fn print_note(build: &Build) {
    if let Some(note) = build.note {
        println!("Note: {note}");
    }
}

fn describe(status: &SettingsStatus) -> String {
    match status.drift.len() {
        0 => "in sync".into(),
        drifted => format!("{drifted} to apply"),
    }
}

fn print_drift(status: &SettingsStatus, indent: &str) {
    for drift in &status.drift {
        let setting = drift.setting;
        println!(
            "{indent}[{}] {}: {} -> {}",
            setting.section,
            setting.key,
            drift.current.as_deref().unwrap_or("(unset)"),
            drift.wanted
        );
    }
}

fn platform() -> String {
    format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

fn field(label: &str, value: &str) {
    println!("{label:<19}{value}");
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn join_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
