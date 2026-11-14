use std::path::PathBuf;

use anyhow::{Context, Result};

/// An app lum installs from its upstream GitHub releases, and the settings it
/// keeps at lum's values.
pub(crate) struct AppSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub owner: &'static str,
    pub repo: &'static str,
    /// Release assets per platform. Without a build for this machine the app is
    /// unavailable here.
    pub builds: &'static [Build],
    pub settings: Option<SettingsSpec>,
}

/// One release asset and how to install it.
pub(crate) struct Build {
    /// A `std::env::consts::OS` value.
    pub os: &'static str,
    /// The `std::env::consts::ARCH` values the asset runs on.
    pub arch: &'static [&'static str],
    /// The release asset's name, with `{version}` standing for the version.
    pub asset: &'static str,
    pub package: Package,
    /// The app's process name; lum leaves a running app alone.
    pub process: &'static str,
}

pub(crate) enum Package {
    /// A Linux AppImage, unpacked into a folder and started by a desktop entry.
    AppImage,
    /// A macOS disk image holding `bundle`.
    DiskImage { bundle: &'static str },
    /// A 7z archive with one top-level folder holding `program`, minus the
    /// files in `remove`.
    SevenZip {
        program: &'static str,
        remove: &'static [&'static str],
    },
}

/// Keys lum owns in an app's INI config file. Every other key stays the app's.
pub(crate) struct SettingsSpec {
    pub file: fn() -> Result<PathBuf>,
    pub values: &'static [Setting],
}

pub(crate) struct Setting {
    pub section: &'static str,
    pub key: &'static str,
    pub value: &'static str,
}

pub(crate) const CATALOG: &[AppSpec] = &[AppSpec {
    name: "mgba",
    description: "Game Boy Advance emulator",
    owner: "mgba-emu",
    repo: "mgba",
    builds: &[
        Build {
            os: "linux",
            arch: &["x86_64"],
            asset: "mGBA-{version}-appimage-x64.appimage",
            package: Package::AppImage,
            process: "mgba-qt",
        },
        Build {
            os: "linux",
            arch: &["aarch64"],
            asset: "mGBA-{version}-appimage-arm64.appimage",
            package: Package::AppImage,
            process: "mgba-qt",
        },
        Build {
            os: "macos",
            arch: &["aarch64", "x86_64"],
            asset: "mGBA-{version}-macos.dmg",
            package: Package::DiskImage { bundle: "mGBA.app" },
            process: "mGBA",
        },
        Build {
            os: "windows",
            arch: &["x86_64"],
            asset: "mGBA-{version}-win64.7z",
            // portable.ini makes mGBA keep its settings beside mGBA.exe, where an
            // update replaces them; without it they live in %APPDATA%\mGBA.
            package: Package::SevenZip {
                program: "mGBA.exe",
                remove: &["portable.ini"],
            },
            process: "mGBA.exe",
        },
    ],
    settings: Some(SettingsSpec {
        file: mgba_config_file,
        values: &[
            // lum installs and updates mGBA; its own updater would replace lum's
            // files behind its back.
            Setting {
                section: "ports.qt",
                key: "updateAutoCheck",
                value: "0",
            },
        ],
    }),
}];

/// mGBA reads config.ini from %APPDATA%\mGBA on Windows and from the XDG config
/// folder everywhere else, macOS included (`mCoreConfigDirectory` upstream).
fn mgba_config_file() -> Result<PathBuf> {
    let dir = if cfg!(windows) {
        crate::paths::roaming_app_data()?.join("mGBA")
    } else {
        crate::paths::xdg_config_home()?.join("mgba")
    };
    Ok(dir.join("config.ini"))
}

impl AppSpec {
    /// The build for this machine, if upstream ships one.
    pub(crate) fn build(&self) -> Option<&'static Build> {
        self.build_for(std::env::consts::OS, std::env::consts::ARCH)
    }

    pub(crate) fn build_for(&self, os: &str, arch: &str) -> Option<&'static Build> {
        self.builds
            .iter()
            .find(|build| build.os == os && build.arch.contains(&arch))
    }

    /// The build for this machine, or an error naming the platform.
    pub(crate) fn require_build(&self) -> Result<&'static Build> {
        self.build().with_context(|| {
            format!(
                "{} has no build for {}/{}",
                self.name,
                std::env::consts::OS,
                std::env::consts::ARCH
            )
        })
    }
}

impl Build {
    /// The process to look for; tests point it at a process they control.
    pub(crate) fn process_name(&self, spec: &AppSpec) -> String {
        std::env::var(test_env("PROCESS", spec)).unwrap_or_else(|_| self.process.to_owned())
    }
}

pub(crate) fn test_artifact_env(spec: &AppSpec) -> String {
    test_env("ARTIFACT", spec)
}

fn test_env(kind: &str, spec: &AppSpec) -> String {
    format!(
        "LUM_APPS_TEST_{kind}_{}",
        spec.name.replace('-', "_").to_ascii_uppercase()
    )
}

pub(crate) fn lookup_app(name: &str) -> Result<&'static AppSpec> {
    CATALOG
        .iter()
        .find(|app| app.name == name)
        .with_context(|| {
            format!(
                "unknown managed app {name:?} (available: {})",
                available_apps()
            )
        })
}

pub(crate) fn available_apps() -> String {
    CATALOG
        .iter()
        .map(|app| app.name)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mgba_has_a_build_for_each_desktop_platform() {
        let mgba = lookup_app("mgba").unwrap();
        let asset = |os, arch| mgba.build_for(os, arch).map(|build| build.asset);

        assert_eq!(
            asset("linux", "x86_64"),
            Some("mGBA-{version}-appimage-x64.appimage")
        );
        assert_eq!(
            asset("linux", "aarch64"),
            Some("mGBA-{version}-appimage-arm64.appimage")
        );
        assert_eq!(asset("macos", "aarch64"), Some("mGBA-{version}-macos.dmg"));
        assert_eq!(asset("macos", "x86_64"), Some("mGBA-{version}-macos.dmg"));
        assert_eq!(asset("windows", "x86_64"), Some("mGBA-{version}-win64.7z"));
        assert_eq!(asset("windows", "aarch64"), None);
        assert_eq!(asset("freebsd", "x86_64"), None);
    }
}
