//! Settings lum owns inside an app's INI config file.

use std::{
    fs,
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use ini::{EscapePolicy, Ini, LineSeparator, ParseOption, WriteOption};

use super::catalog::{Setting, SettingsSpec};

/// An owned setting whose value in the file is not lum's.
pub(crate) struct Drift {
    pub setting: &'static Setting,
    pub current: Option<String>,
}

pub(crate) struct SettingsStatus {
    pub file: PathBuf,
    pub drift: Vec<Drift>,
}

pub(crate) fn status(spec: &SettingsSpec) -> Result<SettingsStatus> {
    let file = (spec.file)()?;
    let drift = drift(&load(&file)?, spec.values);
    Ok(SettingsStatus { file, drift })
}

fn drift(ini: &Ini, values: &'static [Setting]) -> Vec<Drift> {
    values
        .iter()
        .filter_map(|setting| {
            let current = ini.get_from(Some(setting.section), setting.key);
            (current != Some(setting.value)).then(|| Drift {
                setting,
                current: current.map(str::to_owned),
            })
        })
        .collect()
}

/// Write lum's values over the drifted settings. Every other key, and every
/// section lum does not own, stays as the app wrote it.
pub(crate) fn apply(status: &SettingsStatus) -> Result<()> {
    let mut ini = load(&status.file)?;
    for drift in &status.drift {
        ini.with_section(Some(drift.setting.section))
            .set(drift.setting.key, drift.setting.value);
    }
    let mut contents = Vec::new();
    ini.write_to_opt(
        &mut contents,
        WriteOption {
            escape_policy: EscapePolicy::Nothing,
            line_separator: LineSeparator::CR,
            kv_separator: "=",
        },
    )?;
    replace(&status.file, &contents)
        .with_context(|| format!("failed to write {}", status.file.display()))
}

/// Values are taken literally: they hold Windows paths, whose backslashes are
/// not escapes.
fn load(file: &Path) -> Result<Ini> {
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Ini::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", file.display()));
        }
    };
    let option = ParseOption {
        enabled_quote: false,
        enabled_escape: false,
        ..ParseOption::default()
    };
    Ini::load_from_str_opt(&text, option)
        .with_context(|| format!("failed to parse {}", file.display()))
}

/// Swap the new contents in with one rename, so the app never reads half a file.
fn replace(file: &Path, contents: &[u8]) -> Result<()> {
    let dir = file.parent().context("config file has no folder")?;
    fs::create_dir_all(dir)?;
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    temp.write_all(contents)?;
    if let Ok(metadata) = fs::metadata(file) {
        temp.as_file().set_permissions(metadata.permissions())?;
    }
    temp.persist(file).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALUES: &[Setting] = &[Setting {
        section: "ports.qt",
        key: "updateAutoCheck",
        value: "0",
    }];

    fn status_of(file: &Path) -> SettingsStatus {
        SettingsStatus {
            file: file.to_owned(),
            drift: drift(&load(file).unwrap(), VALUES),
        }
    }

    #[test]
    fn apply_sets_owned_keys_and_keeps_everything_else() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.ini");
        fs::write(
            &file,
            "root=1\n[ports.qt]\nupdateAutoCheck=1\nbios=C:\\Games\\gba_bios.bin\nshader=\n\n[gba.input.QT_K]\nkeyA=88\n",
        )
        .unwrap();

        let status = status_of(&file);
        assert_eq!(status.drift.len(), 1);
        assert_eq!(status.drift[0].current.as_deref(), Some("1"));
        apply(&status).unwrap();

        let ini = load(&file).unwrap();
        assert_eq!(ini.get_from(None::<&str>, "root"), Some("1"));
        assert_eq!(ini.get_from(Some("ports.qt"), "updateAutoCheck"), Some("0"));
        assert_eq!(ini.get_from(Some("ports.qt"), "shader"), Some(""));
        assert_eq!(ini.get_from(Some("gba.input.QT_K"), "keyA"), Some("88"));
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.contains("bios=C:\\Games\\gba_bios.bin\n"), "{text}");
        assert!(status_of(&file).drift.is_empty());
    }

    #[test]
    fn apply_creates_a_missing_config() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("mgba").join("config.ini");

        let status = status_of(&file);
        assert_eq!(status.drift.len(), 1);
        assert_eq!(status.drift[0].current, None);
        apply(&status).unwrap();

        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "[ports.qt]\nupdateAutoCheck=0\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn apply_keeps_the_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.ini");
        fs::write(&file, "[ports.qt]\n").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();

        apply(&status_of(&file)).unwrap();

        let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
    }
}
