//! Settings lum owns in an app's config file. lum rewrites only the lines of
//! the keys it owns; every other line stays byte for byte as the app wrote it.

use std::{
    fs,
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use super::catalog::{Dialect, Setting, SettingsSpec, Value};

/// An owned setting whose value in the file is not lum's.
pub(crate) struct Drift {
    pub setting: &'static Setting,
    pub current: Option<String>,
    pub wanted: String,
}

pub(crate) struct SettingsStatus {
    pub file: PathBuf,
    pub dialect: Dialect,
    pub drift: Vec<Drift>,
}

/// The owned settings of an app installed at `app` that differ from lum's.
pub(crate) fn status(spec: &'static SettingsSpec, app: &Path) -> Result<SettingsStatus> {
    let file = (spec.file)(app)?;
    let text = read(&file)?;
    let mut drift = Vec::new();
    for setting in spec.values {
        let wanted = setting.value.resolve()?;
        let current = get(&text, spec.dialect, setting.section, setting.key);
        if current.as_deref() != Some(wanted.as_str()) {
            drift.push(Drift {
                setting,
                current,
                wanted,
            });
        }
    }
    Ok(SettingsStatus {
        file,
        dialect: spec.dialect,
        drift,
    })
}

/// Write lum's values over the drifted settings.
pub(crate) fn apply(status: &SettingsStatus) -> Result<()> {
    let mut text = read(&status.file)?;
    for drift in &status.drift {
        if matches!(drift.setting.value, Value::RomsFolder) {
            // An app may not create the folder before it first saves there.
            fs::create_dir_all(&drift.wanted)
                .with_context(|| format!("failed to create {}", drift.wanted))?;
        }
        text = set(
            &text,
            status.dialect,
            drift.setting.section,
            drift.setting.key,
            &drift.wanted,
        );
    }
    replace(&status.file, text.as_bytes())
        .with_context(|| format!("failed to write {}", status.file.display()))
}

impl Value {
    fn resolve(&self) -> Result<String> {
        Ok(match self {
            Value::Text(text) => (*text).to_owned(),
            Value::RomsFolder => crate::paths::roms_dir()?.display().to_string(),
        })
    }
}

impl Dialect {
    /// The key and the raw value of an entry line.
    fn split(self, line: &str) -> Option<(&str, &str)> {
        let at = match self {
            Dialect::Inih => line.find(['=', ':']),
            Dialect::Snes9x => line.find('='),
        }?;
        Some((line[..at].trim(), &line[at + 1..]))
    }

    /// A raw value as the app reads it.
    fn decode(self, raw: &str) -> String {
        match self {
            Dialect::Inih => raw.trim().to_owned(),
            Dialect::Snes9x => {
                let mut value = String::new();
                let mut chars = raw.trim_start().chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '#' && chars.next_if_eq(&'#').is_none() {
                        break;
                    }
                    value.push(c);
                }
                let value = value.trim_end();
                value
                    .strip_prefix('"')
                    .and_then(|inner| inner.strip_suffix('"'))
                    .unwrap_or(value)
                    .to_owned()
            }
        }
    }

    /// An entry line, written the way the app writes it.
    fn line(self, key: &str, value: &str) -> String {
        match self {
            Dialect::Inih => format!("{key}={value}"),
            Dialect::Snes9x => {
                // Quotes keep outer spaces, and stop a trailing backslash from
                // continuing the entry on the next line.
                let value = if value.trim() != value || value.ends_with('\\') {
                    format!("\"{value}\"")
                } else {
                    value.to_owned()
                };
                format!("{key} = {}", value.replace('#', "##"))
            }
        }
    }
}

/// The value of `key` in `[section]`. The last one counts, as in mGBA and snes9x.
fn get(text: &str, dialect: Dialect, section: &str, key: &str) -> Option<String> {
    let mut current = None;
    let mut value = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = section_name(line) {
            current = Some(name);
        } else if current == Some(section)
            && !is_comment(line)
            && let Some((found, raw)) = dialect.split(line)
            && found == key
        {
            value = Some(dialect.decode(raw));
        }
    }
    value
}

/// `text` with every `key` line in `[section]` set to `value`, or with a new line
/// after the section's last entry, or a new section at the end. Lines keep
/// their own endings; new ones copy the file's.
fn set(text: &str, dialect: Dialect, section: &str, key: &str, value: &str) -> String {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let entry = dialect.line(key, value);
    let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_owned).collect();
    let mut current: Option<String> = None;
    let mut last_in_section = None;
    let mut found = false;
    for (index, line) in lines.iter_mut().enumerate() {
        let trimmed = line.trim();
        if let Some(name) = section_name(trimmed) {
            if name == section {
                last_in_section = Some(index);
            }
            current = Some(name.to_owned());
            continue;
        }
        if current.as_deref() != Some(section) || trimmed.is_empty() || is_comment(trimmed) {
            continue;
        }
        last_in_section = Some(index);
        if dialect
            .split(trimmed)
            .is_some_and(|(found, _)| found == key)
        {
            let ending = line[line.trim_end_matches(['\r', '\n']).len()..].to_owned();
            *line = format!("{entry}{ending}");
            found = true;
        }
    }
    if !found {
        match last_in_section {
            Some(index) => {
                if !lines[index].ends_with('\n') {
                    lines[index].push_str(newline);
                }
                lines.insert(index + 1, format!("{entry}{newline}"));
            }
            None => {
                if let Some(last) = lines.last_mut() {
                    if !last.ends_with('\n') {
                        last.push_str(newline);
                    }
                    lines.push(newline.to_owned());
                }
                lines.push(format!("[{section}]{newline}"));
                lines.push(format!("{entry}{newline}"));
            }
        }
    }
    lines.concat()
}

fn section_name(line: &str) -> Option<&str> {
    line.strip_prefix('[')?.strip_suffix(']').map(str::trim)
}

fn is_comment(line: &str) -> bool {
    line.starts_with('#') || line.starts_with(';')
}

/// The file's text, or nothing if it does not exist yet. A file that is not
/// UTF-8 is refused rather than rewritten lossily.
fn read(file: &Path) -> Result<String> {
    match fs::read_to_string(file) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", file.display())),
    }
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

    const MGBA: &str = "root=1\n[ports.qt]\nupdateAutoCheck=1\nbios=C:\\Games\\gba_bios.bin\nshader=\n\n[gba.input.QT_K]\nkeyA=88\n";

    /// How snes9x for Windows writes its config: CRLF, keys with colons,
    /// trailing comments.
    const SNES9X: &str = "# Config file output by snes9x\r\n\r\n[Settings\\Win\\Files]\r\nDir:Bios = .\\BIOS  # directory where BIOS files will be located\r\nDir:SRAM = .\\Saves  # directory where battery saves will be created and loaded from\r\nDir:Savestates = .\\Saves  # directory where savestates will be created and loaded from\r\n\r\n[Display\\Win]\r\nOutputMethod = 1  # 0=DirectDraw, 1=Direct3D, 2=OpenGL\r\n";

    #[test]
    fn inih_values_are_read_and_set_in_place() {
        assert_eq!(
            get(MGBA, Dialect::Inih, "ports.qt", "updateAutoCheck").as_deref(),
            Some("1")
        );
        assert_eq!(
            get(MGBA, Dialect::Inih, "gba.input.QT_K", "keyA").as_deref(),
            Some("88")
        );
        assert_eq!(
            get("[a]\nkey: 7\n", Dialect::Inih, "a", "key").as_deref(),
            Some("7")
        );

        let text = set(MGBA, Dialect::Inih, "ports.qt", "updateAutoCheck", "0");
        assert_eq!(text, MGBA.replace("updateAutoCheck=1", "updateAutoCheck=0"));
    }

    #[test]
    fn a_missing_key_goes_after_its_section_entries() {
        let text = set(MGBA, Dialect::Inih, "ports.qt", "showLibrary", "0");
        assert_eq!(text, MGBA.replace("shader=\n", "shader=\nshowLibrary=0\n"));
    }

    #[test]
    fn a_missing_section_goes_at_the_end() {
        assert_eq!(
            set("", Dialect::Inih, "ports.qt", "updateAutoCheck", "0"),
            "[ports.qt]\nupdateAutoCheck=0\n"
        );
        assert_eq!(
            set(
                "[a]\nx=1",
                Dialect::Inih,
                "ports.qt",
                "updateAutoCheck",
                "0"
            ),
            "[a]\nx=1\n\n[ports.qt]\nupdateAutoCheck=0\n"
        );
    }

    #[test]
    fn duplicate_keys_are_all_set_and_the_last_counts() {
        let text = "[a]\nx=1\n[b]\nx=9\n[a]\nx=2\n";
        assert_eq!(get(text, Dialect::Inih, "a", "x").as_deref(), Some("2"));
        assert_eq!(
            set(text, Dialect::Inih, "a", "x", "3"),
            "[a]\nx=3\n[b]\nx=9\n[a]\nx=3\n"
        );
    }

    #[test]
    fn snes9x_keys_keep_their_colon_and_comments_are_not_values() {
        assert_eq!(
            get(SNES9X, Dialect::Snes9x, r"Settings\Win\Files", "Dir:SRAM").as_deref(),
            Some(r".\Saves")
        );
        assert_eq!(
            get(SNES9X, Dialect::Snes9x, r"Display\Win", "OutputMethod").as_deref(),
            Some("1")
        );

        let text = set(
            SNES9X,
            Dialect::Snes9x,
            r"Settings\Win\Files",
            "Dir:SRAM",
            r"C:\Users\me\ROMs",
        );
        assert_eq!(
            text,
            SNES9X.replace(
                "Dir:SRAM = .\\Saves  # directory where battery saves will be created and loaded from\r\n",
                "Dir:SRAM = C:\\Users\\me\\ROMs\r\n"
            )
        );
        assert_eq!(
            get(&text, Dialect::Snes9x, r"Settings\Win\Files", "Dir:SRAM").as_deref(),
            Some(r"C:\Users\me\ROMs")
        );
    }

    #[test]
    fn snes9x_values_round_trip_hashes_spaces_and_trailing_backslashes() {
        for value in ["a#b", " padded ", r"C:\", "plain"] {
            let line = Dialect::Snes9x.line("Key", value);
            let (_, raw) = Dialect::Snes9x.split(&line).unwrap();
            assert_eq!(Dialect::Snes9x.decode(raw), value, "{line}");
        }
        assert_eq!(Dialect::Snes9x.line("Key", "a#b"), "Key = a##b");
    }

    #[test]
    fn apply_creates_a_missing_config() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("mgba").join("config.ini");
        static SETTING: Setting = Setting {
            section: "ports.qt",
            key: "updateAutoCheck",
            value: Value::Text("0"),
        };
        let status = SettingsStatus {
            file: file.clone(),
            dialect: Dialect::Inih,
            drift: vec![Drift {
                setting: &SETTING,
                current: None,
                wanted: "0".into(),
            }],
        };

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
        static SETTING: Setting = Setting {
            section: "ports.qt",
            key: "updateAutoCheck",
            value: Value::Text("0"),
        };
        let status = SettingsStatus {
            file: file.clone(),
            dialect: Dialect::Inih,
            drift: vec![Drift {
                setting: &SETTING,
                current: None,
                wanted: "0".into(),
            }],
        };

        apply(&status).unwrap();

        let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "[ports.qt]\nupdateAutoCheck=0\n"
        );
    }
}
