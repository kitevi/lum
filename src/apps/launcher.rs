//! Making installed apps start like any other desktop app.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};

use super::catalog::{AppSpec, Package};

/// The launcher lum writes for an app, if its platform needs one. macOS needs
/// none: Spotlight and Launchpad find apps in ~/Applications on their own.
pub(crate) fn path(spec: &AppSpec, package: &Package) -> Result<Option<PathBuf>> {
    Ok(match package {
        Package::AppImage => Some(applications_dir()?.join(format!("lum-{}.desktop", spec.name))),
        Package::SevenZip { program, .. } => {
            Some(start_menu_dir()?.join(format!("{}.lnk", program.trim_end_matches(".exe"))))
        }
        Package::DiskImage { .. } => None,
    })
}

/// Write the launcher for the app installed at `app`.
pub(crate) fn install(spec: &AppSpec, package: &Package, app: &Path) -> Result<()> {
    let Some(launcher) = path(spec, package)? else {
        return Ok(());
    };
    if let Some(dir) = launcher.parent() {
        fs::create_dir_all(dir)?;
    }
    match package {
        Package::AppImage => {
            fs::write(&launcher, desktop_entry(app)?)?;
            refresh_desktop_database();
        }
        Package::SevenZip { program, .. } => {
            start_menu_shortcut(&launcher, &app.join(program), app)?;
        }
        Package::DiskImage { .. } => {}
    }
    Ok(())
}

pub(crate) fn remove(spec: &AppSpec, package: &Package) -> Result<()> {
    if let Some(launcher) = path(spec, package)? {
        super::remove_file_if_present(&launcher)?;
        if matches!(package, Package::AppImage) {
            refresh_desktop_database();
        }
    }
    Ok(())
}

fn applications_dir() -> Result<PathBuf> {
    Ok(crate::paths::xdg_data_home()?.join("applications"))
}

fn start_menu_dir() -> Result<PathBuf> {
    Ok(crate::paths::roaming_app_data()?
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("lum"))
}

/// The AppImage's own desktop entry, which keeps its file associations, pointed
/// at the unpacked app. The format puts exactly one entry at the image root.
fn desktop_entry(app: &Path) -> Result<String> {
    let source = fs::read_dir(app)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "desktop"))
        .with_context(|| format!("no desktop entry in {}", app.display()))?;
    if !fs::canonicalize(&source)?.starts_with(fs::canonicalize(app)?) {
        bail!("{} points outside the app", source.display());
    }
    let entry = fs::read_to_string(&source)?;
    let icon = entry
        .lines()
        .find_map(|line| line.strip_prefix("Icon="))
        .and_then(|name| icon_file(app, name.trim()));
    Ok(rewrite_desktop_entry(
        &entry,
        &app.join("AppRun"),
        icon.as_deref(),
    ))
}

/// The icon file an entry names; AppImages keep it at the image root.
fn icon_file(app: &Path, name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        return None;
    }
    ["png", "svg", "xpm"]
        .iter()
        .map(|ext| app.join(format!("{name}.{ext}")))
        .chain([app.join(".DirIcon")])
        .find(|path| path.is_file())
}

/// Point Exec at `program`, keeping its arguments, and Icon at `icon`. TryExec
/// goes: it names the program the AppImage expected on PATH.
fn rewrite_desktop_entry(entry: &str, program: &Path, icon: Option<&Path>) -> String {
    let mut out = String::new();
    for line in entry.lines() {
        if let Some(command) = line.strip_prefix("Exec=") {
            out.push_str("Exec=");
            out.push_str(&exec_arg(program));
            let args = exec_args(command);
            if !args.is_empty() {
                out.push(' ');
                out.push_str(args);
            }
        } else if line.starts_with("TryExec=") {
            continue;
        } else if let Some(icon) = icon.filter(|_| line.starts_with("Icon=")) {
            out.push_str("Icon=");
            out.push_str(&icon.to_string_lossy());
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// The arguments after the program in an Exec value.
fn exec_args(command: &str) -> &str {
    let command = command.trim_start();
    let rest = match command.strip_prefix('"') {
        Some(quoted) => quoted.find('"').map_or("", |end| &quoted[end + 1..]),
        None => command
            .find(char::is_whitespace)
            .map_or("", |end| &command[end..]),
    };
    rest.trim()
}

/// A path as one Exec argument. Per the Desktop Entry spec a quoted argument
/// escapes double quotes, dollar signs, backticks and backslashes with a
/// backslash, string escaping then doubles every backslash, and a literal `%`
/// is written `%%`.
fn exec_arg(path: &Path) -> String {
    let path = path.to_string_lossy();
    if path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._+-".contains(c))
    {
        return path.into_owned();
    }
    let mut quoted = String::from('"');
    for c in path.chars() {
        match c {
            '"' | '$' | '`' => {
                quoted.push_str(r"\\");
                quoted.push(c);
            }
            '\\' => quoted.push_str(r"\\\\"),
            '%' => quoted.push_str("%%"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// A Start Menu shortcut, written by Windows' own WScript.Shell (as Scoop does)
/// rather than by hand-encoding the binary .lnk format.
fn start_menu_shortcut(link: &Path, target: &Path, working_dir: &Path) -> Result<()> {
    let quote = |path: &Path| crate::env::shell::powershell_quote(&path.to_string_lossy());
    let script = format!(
        "$link = (New-Object -ComObject WScript.Shell).CreateShortcut({}); \
         $link.TargetPath = {}; $link.WorkingDirectory = {}; $link.Save()",
        quote(link),
        quote(target),
        quote(working_dir)
    );
    super::run_tool(Command::new("powershell").args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &script,
    ]))
}

/// Best effort: refresh the MIME cache so "Open With" offers the app.
fn refresh_desktop_database() {
    if let Ok(dir) = applications_dir() {
        let _ = Command::new("update-desktop-database")
            .arg(dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = "[Desktop Entry]\nName=mGBA\nIcon=io.mgba.mGBA\nTryExec=mgba-qt\nExec=mgba-qt %f\nMimeType=application/x-gba-rom;\n";

    #[test]
    fn desktop_entry_runs_the_unpacked_app() {
        let entry = rewrite_desktop_entry(
            ENTRY,
            Path::new("/apps/mgba/AppRun"),
            Some(Path::new("/apps/mgba/io.mgba.mGBA.png")),
        );
        assert_eq!(
            entry,
            "[Desktop Entry]\nName=mGBA\nIcon=/apps/mgba/io.mgba.mGBA.png\nExec=/apps/mgba/AppRun %f\nMimeType=application/x-gba-rom;\n"
        );
    }

    #[test]
    fn desktop_entry_keeps_an_icon_name_it_cannot_resolve() {
        let entry = rewrite_desktop_entry(ENTRY, Path::new("/apps/mgba/AppRun"), None);
        assert!(entry.contains("\nIcon=io.mgba.mGBA\n"));
    }

    #[test]
    fn exec_arguments_follow_the_program() {
        assert_eq!(exec_args("mgba-qt %f"), "%f");
        assert_eq!(exec_args("\"my app\" --flag %U"), "--flag %U");
        assert_eq!(exec_args("app"), "");
    }

    #[test]
    fn exec_quotes_paths_with_reserved_characters() {
        assert_eq!(
            exec_arg(Path::new("/home/me/.local/share/lum/apps/mgba/AppRun")),
            "/home/me/.local/share/lum/apps/mgba/AppRun"
        );
        assert_eq!(
            exec_arg(Path::new("/home/my games/AppRun")),
            "\"/home/my games/AppRun\""
        );
        assert_eq!(exec_arg(Path::new("/a$b/100%")), r#""/a\\$b/100%%""#);
        assert_eq!(exec_arg(Path::new(r"/a\b")), r#""/a\\\\b""#);
    }
}
