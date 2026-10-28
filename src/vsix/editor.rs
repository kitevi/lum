use std::{
    collections::BTreeMap,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result};

use crate::cli::VsixEditor;

const EDITORS: [VsixEditor; 3] = [VsixEditor::Code, VsixEditor::Cursor, VsixEditor::Codium];

/// A VS Code-compatible editor CLI. The editor owns its extensions directory and
/// registry, so lum installs and inspects extensions only through this CLI.
pub(crate) struct Editor {
    pub name: String,
    pub path: PathBuf,
}

/// Resolve the editor CLI: $LUM_VSIX_EDITOR, else the requested editor, else the
/// first of code, cursor, codium found on PATH or in its default install location.
pub(crate) fn resolve(choice: Option<VsixEditor>) -> Result<Editor> {
    if let Some(path) = std::env::var_os("LUM_VSIX_EDITOR").filter(|value| !value.is_empty()) {
        let path = PathBuf::from(path);
        let name = match choice {
            Some(editor) => command_name(editor).to_owned(),
            None => path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| "editor".to_owned()),
        };
        return Ok(Editor { name, path });
    }
    match choice {
        Some(editor) => locate(editor).with_context(|| {
            format!(
                "{} CLI not found on PATH or in its default install location",
                command_name(editor)
            )
        }),
        None => EDITORS.into_iter().find_map(locate).context(
            "no editor CLI found (looked for code, cursor, and codium on PATH and in default install locations)",
        ),
    }
}

fn command_name(editor: VsixEditor) -> &'static str {
    match editor {
        VsixEditor::Code => "code",
        VsixEditor::Cursor => "cursor",
        VsixEditor::Codium => "codium",
    }
}

fn locate(editor: VsixEditor) -> Option<Editor> {
    let name = command_name(editor);
    which::which(name)
        .ok()
        .or_else(|| {
            install_locations(editor)
                .into_iter()
                .find(|path| path.is_file())
        })
        .map(|path| Editor {
            name: name.to_owned(),
            path,
        })
}

/// Where the CLI lives when the installer did not put it on PATH.
#[cfg(target_os = "macos")]
fn install_locations(editor: VsixEditor) -> Vec<PathBuf> {
    let (bundle, bin) = match editor {
        VsixEditor::Code => ("Visual Studio Code.app", "code"),
        VsixEditor::Cursor => ("Cursor.app", "cursor"),
        VsixEditor::Codium => ("VSCodium.app", "codium"),
    };
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Ok(home) = crate::paths::home_dir() {
        roots.push(home.join("Applications"));
    }
    roots
        .into_iter()
        .map(|root| {
            root.join(bundle)
                .join("Contents/Resources/app/bin")
                .join(bin)
        })
        .collect()
}

#[cfg(windows)]
fn install_locations(editor: VsixEditor) -> Vec<PathBuf> {
    let (dir, bin) = match editor {
        VsixEditor::Code => ("Microsoft VS Code", r"bin\code.cmd"),
        VsixEditor::Cursor => ("cursor", r"resources\app\bin\cursor.cmd"),
        VsixEditor::Codium => ("VSCodium", r"bin\codium.cmd"),
    };
    let mut roots = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("Programs"));
    }
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        roots.push(PathBuf::from(program_files));
    }
    roots
        .into_iter()
        .map(|root| root.join(dir).join(bin))
        .collect()
}

#[cfg(not(any(target_os = "macos", windows)))]
fn install_locations(_editor: VsixEditor) -> Vec<PathBuf> {
    Vec::new()
}

impl Editor {
    /// Installed extensions keyed by lowercase extension id.
    pub(crate) fn installed_extensions(&self) -> Result<BTreeMap<String, String>> {
        let stdout = self.run(&[
            OsStr::new("--list-extensions"),
            OsStr::new("--show-versions"),
        ])?;
        Ok(parse_extension_list(&stdout))
    }

    pub(crate) fn install(&self, vsix: &Path) -> Result<()> {
        self.run(&[
            OsStr::new("--install-extension"),
            vsix.as_os_str(),
            OsStr::new("--force"),
        ])?;
        Ok(())
    }

    fn run(&self, args: &[&OsStr]) -> Result<String> {
        let output = Command::new(&self.path)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .with_context(|| format!("failed to run {}", self.path.display()))?;
        if !output.status.success() {
            anyhow::bail!(
                "{} {} failed ({}): {}",
                self.name,
                args.first()
                    .map_or_else(String::new, |arg| arg.to_string_lossy().into_owned()),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

pub(crate) fn installed_version<'a>(
    installed: &'a BTreeMap<String, String>,
    extension_id: &str,
) -> Option<&'a str> {
    installed
        .get(&extension_id.to_ascii_lowercase())
        .map(String::as_str)
}

/// Parse `--list-extensions --show-versions` output, ignoring any line that is
/// not `publisher.name@version` (Electron can print warnings around the list).
fn parse_extension_list(stdout: &str) -> BTreeMap<String, String> {
    let token = |value: &str| !value.is_empty() && !value.contains(char::is_whitespace);
    stdout
        .lines()
        .filter_map(|line| {
            let (id, version) = line.trim().split_once('@')?;
            (token(id) && id.contains('.') && token(version))
                .then(|| (id.to_ascii_lowercase(), version.to_owned()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extension_list_and_skips_noise() {
        let installed = parse_extension_list(
            "Warning: 'enable-features' is not in the list of known options\n\
             ag-libs.lathe@0.1.24\r\n\
             RedHat.java@1.58.0\n\
             \n\
             not an extension\n\
             missing-dot@1.0.0\n\
             broken.version@\n",
        );
        assert_eq!(installed.len(), 2);
        assert_eq!(
            installed_version(&installed, "ag-libs.lathe"),
            Some("0.1.24")
        );
        assert_eq!(installed_version(&installed, "redhat.java"), Some("1.58.0"));
        assert_eq!(installed_version(&installed, "REDHAT.JAVA"), Some("1.58.0"));
    }
}
