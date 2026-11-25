use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// An app lum installs from its upstream releases.
pub(crate) struct AppSpec {
    pub name: &'static str,
    /// The app's own name, for the launchers lum names itself.
    pub title: &'static str,
    pub description: &'static str,
    pub source: Source,
    /// Release assets per platform. The app shows up only on the OSes it has a
    /// build for, and is unavailable on a CPU without one.
    pub builds: &'static [Build],
}

/// Where upstream publishes its releases.
pub(crate) enum Source {
    GitHub {
        owner: &'static str,
        repo: &'static str,
    },
    /// A gitlab.com project, by its path.
    GitLab { project: &'static str },
}

/// One release asset, how to install it, and the settings lum keeps for it.
pub(crate) struct Build {
    /// A `std::env::consts::OS` value.
    pub os: &'static str,
    /// The `std::env::consts::ARCH` values the asset runs on.
    pub arch: &'static [&'static str],
    /// The release asset's name, with `{version}` standing for the version. Its
    /// extension says how it is packed: `.dmg`, `.zip` or `.7z` for bundles and
    /// program folders.
    pub asset: &'static str,
    pub package: Package,
    /// The app's process name; lum leaves a running app alone.
    pub process: &'static str,
    pub settings: Option<&'static SettingsSpec>,
    /// Shown once after installing, for what lum cannot do itself.
    pub note: Option<&'static str>,
}

pub(crate) enum Package {
    /// A Linux AppImage, unpacked into a folder and started by a desktop entry.
    AppImage,
    /// A macOS app bundle, from a disk image or a zip.
    Bundle { bundle: &'static str },
    /// A Windows program folder from a zip or 7z archive: the archive's only
    /// folder, or its root. `remove` lists files dropped after unpacking. `keep`
    /// lists what the program writes beside itself, which an update carries over
    /// from the previous install.
    Folder {
        program: &'static str,
        remove: &'static [&'static str],
        keep: &'static [&'static str],
    },
}

/// Keys lum owns in an app's config file. Every other line stays the app's.
pub(crate) struct SettingsSpec {
    /// The config file, given the folder the app is installed in.
    pub file: fn(&Path) -> Result<PathBuf>,
    pub dialect: Dialect,
    pub values: &'static [Setting],
}

/// How an app reads its config file.
#[derive(Clone, Copy)]
pub(crate) enum Dialect {
    /// inih, as mGBA uses it: `key=value`, split at the first `=` or `:`.
    Inih,
    /// snes9x's ConfigFile: `key = value`, split at the first `=` only, so keys
    /// such as `Dir:SRAM` keep their colon. A `#` starts a comment and `##` is a
    /// literal `#`; quotes keep a value's outer spaces.
    Snes9x,
}

pub(crate) struct Setting {
    pub section: &'static str,
    pub key: &'static str,
    pub value: Value,
}

pub(crate) enum Value {
    Text(&'static str),
    /// The ROMs folder that `lum backup create roms` backs up, so saves kept there are
    /// backed up with the games.
    RomsFolder,
}

const CATALOG: &[AppSpec] = &[
    AppSpec {
        name: "mgba",
        title: "mGBA",
        description: "Game Boy Advance emulator",
        source: Source::GitHub {
            owner: "mgba-emu",
            repo: "mgba",
        },
        builds: &[
            Build {
                os: "linux",
                arch: &["x86_64"],
                asset: "mGBA-{version}-appimage-x64.appimage",
                package: Package::AppImage,
                process: "mgba-qt",
                settings: Some(&MGBA_SETTINGS),
                note: None,
            },
            Build {
                os: "linux",
                arch: &["aarch64"],
                asset: "mGBA-{version}-appimage-arm64.appimage",
                package: Package::AppImage,
                process: "mgba-qt",
                settings: Some(&MGBA_SETTINGS),
                note: None,
            },
            Build {
                os: "macos",
                arch: &["aarch64", "x86_64"],
                asset: "mGBA-{version}-macos.dmg",
                package: Package::Bundle { bundle: "mGBA.app" },
                process: "mGBA",
                settings: Some(&MGBA_SETTINGS),
                note: None,
            },
            Build {
                os: "windows",
                arch: &["x86_64"],
                asset: "mGBA-{version}-win64.7z",
                // portable.ini makes mGBA keep its settings beside mGBA.exe, where an
                // update replaces them; without it they live in %APPDATA%\mGBA.
                package: Package::Folder {
                    program: "mGBA.exe",
                    remove: &["portable.ini"],
                    keep: &[],
                },
                process: "mGBA.exe",
                settings: Some(&MGBA_SETTINGS),
                note: None,
            },
        ],
    },
    AppSpec {
        name: "octomac",
        title: "OctoMacLauncher",
        description: "OctoWoW launcher",
        source: Source::GitLab {
            project: "noinstances/octomac",
        },
        // Apple Silicon only. It keeps its settings in user defaults
        // (local.octomaclauncher) rather than a file, so lum owns none, and
        // writes nothing inside its bundle.
        builds: &[Build {
            os: "macos",
            arch: &["aarch64"],
            asset: "OctoMacLauncher-{version}-macOS-arm64.zip",
            package: Package::Bundle {
                bundle: "OctoMacLauncher.app",
            },
            process: "OctoMacLauncher",
            settings: None,
            note: None,
        }],
    },
    AppSpec {
        name: "snes9x",
        title: "Snes9x",
        description: "Super Nintendo emulator",
        source: Source::GitHub {
            owner: "snes9xgit",
            repo: "snes9x",
        },
        builds: &[
            // Saves land beside each ROM unless the GTK build is told otherwise.
            Build {
                os: "linux",
                arch: &["x86_64"],
                asset: "Snes9x-{version}-x86_64.AppImage",
                package: Package::AppImage,
                process: "snes9x-gtk",
                settings: None,
                // Upstream's AppImage leaves glibmm out along with GLib itself.
                note: Some(
                    "Snes9x needs glibmm 2.4 from the system (libglibmm-2.4.so.1); on Arch: sudo pacman -S glibmm",
                ),
            },
            // The Mac build keeps its preferences in user defaults, not a file, and
            // always saves to ~/Library/Application Support/Snes9x.
            Build {
                os: "macos",
                arch: &["aarch64", "x86_64"],
                asset: "Snes9x-{version}-Mac.zip",
                package: Package::Bundle {
                    bundle: "Snes9x.app",
                },
                process: "Snes9x",
                settings: None,
                note: None,
            },
            Build {
                os: "windows",
                arch: &["x86_64"],
                asset: "snes9x-{version}-win32-x64.zip",
                package: Package::Folder {
                    program: "snes9x-x64.exe",
                    remove: &[],
                    // snes9x.conf, and the folders its `Dir:` settings default to.
                    keep: &[
                        "snes9x.conf",
                        "BIOS",
                        "Cheats",
                        "Movies",
                        "Patches",
                        "Roms",
                        "SatData",
                        "Saves",
                        "Screenshots",
                        "SPCs",
                    ],
                },
                process: "snes9x-x64.exe",
                settings: Some(&SNES9X_WINDOWS_SETTINGS),
                note: None,
            },
        ],
    },
];

const MGBA_SETTINGS: SettingsSpec = SettingsSpec {
    file: mgba_config_file,
    dialect: Dialect::Inih,
    values: &[
        // lum installs and updates mGBA; its own updater would replace lum's files
        // behind its back.
        Setting {
            section: "ports.qt",
            key: "updateAutoCheck",
            value: Value::Text("0"),
        },
    ],
};

/// snes9x for Windows saves into its own folder by default, which an update
/// replaces. lum sends saves to the ROMs folder instead, where they sit with the
/// games as they do on Linux.
const SNES9X_WINDOWS_SETTINGS: SettingsSpec = SettingsSpec {
    file: snes9x_windows_config_file,
    dialect: Dialect::Snes9x,
    values: &[
        Setting {
            section: r"Settings\Win\Files",
            key: "Dir:SRAM",
            value: Value::RomsFolder,
        },
        Setting {
            section: r"Settings\Win\Files",
            key: "Dir:Savestates",
            value: Value::RomsFolder,
        },
    ],
};

/// mGBA reads config.ini from %APPDATA%\mGBA on Windows and from the XDG config
/// folder everywhere else, macOS included (`mCoreConfigDirectory` upstream).
fn mgba_config_file(_app: &Path) -> Result<PathBuf> {
    let dir = if cfg!(windows) {
        crate::paths::roaming_app_data()?.join("mGBA")
    } else {
        crate::paths::xdg_config_home()?.join("mgba")
    };
    Ok(dir.join("config.ini"))
}

/// snes9x for Windows reads snes9x.conf beside its program, always.
fn snes9x_windows_config_file(app: &Path) -> Result<PathBuf> {
    Ok(app.join("snes9x.conf"))
}

impl AppSpec {
    /// Whether upstream builds the app for `os` at all, on any CPU.
    fn built_for_os(&self, os: &str) -> bool {
        self.builds.iter().any(|build| build.os == os)
    }

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

/// The catalog on this OS: the apps upstream builds for it. The others do not
/// show up here, not even as unavailable.
pub(crate) fn apps() -> impl Iterator<Item = &'static AppSpec> {
    CATALOG
        .iter()
        .filter(|app| app.built_for_os(std::env::consts::OS))
}

pub(crate) fn lookup_app(name: &str) -> Result<&'static AppSpec> {
    apps()
        .find(|app| app.name == name)
        .ok_or_else(|| crate::cli::unknown_name("app", name, apps().map(|app| app.name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An app from the whole catalog, whatever OS runs the test.
    fn asset(app: &str, os: &str, arch: &str) -> Option<&'static str> {
        CATALOG
            .iter()
            .find(|spec| spec.name == app)
            .unwrap()
            .build_for(os, arch)
            .map(|build| build.asset)
    }

    #[test]
    fn mgba_has_a_build_for_each_desktop_platform() {
        assert_eq!(
            asset("mgba", "linux", "x86_64"),
            Some("mGBA-{version}-appimage-x64.appimage")
        );
        assert_eq!(
            asset("mgba", "linux", "aarch64"),
            Some("mGBA-{version}-appimage-arm64.appimage")
        );
        assert_eq!(
            asset("mgba", "macos", "aarch64"),
            Some("mGBA-{version}-macos.dmg")
        );
        assert_eq!(
            asset("mgba", "macos", "x86_64"),
            Some("mGBA-{version}-macos.dmg")
        );
        assert_eq!(
            asset("mgba", "windows", "x86_64"),
            Some("mGBA-{version}-win64.7z")
        );
        assert_eq!(asset("mgba", "windows", "aarch64"), None);
        assert_eq!(asset("mgba", "freebsd", "x86_64"), None);
    }

    #[test]
    fn snes9x_has_a_build_for_each_desktop_platform() {
        assert_eq!(
            asset("snes9x", "linux", "x86_64"),
            Some("Snes9x-{version}-x86_64.AppImage")
        );
        assert_eq!(asset("snes9x", "linux", "aarch64"), None);
        assert_eq!(
            asset("snes9x", "macos", "aarch64"),
            Some("Snes9x-{version}-Mac.zip")
        );
        assert_eq!(
            asset("snes9x", "windows", "x86_64"),
            Some("snes9x-{version}-win32-x64.zip")
        );
    }

    #[test]
    fn octomac_has_a_build_for_apple_silicon_macs_only() {
        assert_eq!(
            asset("octomac", "macos", "aarch64"),
            Some("OctoMacLauncher-{version}-macOS-arm64.zip")
        );
        assert_eq!(asset("octomac", "macos", "x86_64"), None);
        assert_eq!(asset("octomac", "linux", "x86_64"), None);
        assert_eq!(asset("octomac", "windows", "x86_64"), None);
    }

    #[test]
    fn apps_show_up_only_on_the_oses_they_are_built_for() {
        let shown = |os: &str| {
            CATALOG
                .iter()
                .filter(|app| app.built_for_os(os))
                .map(|app| app.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(shown("macos"), ["mgba", "octomac", "snes9x"]);
        assert_eq!(shown("linux"), ["mgba", "snes9x"]);
        assert_eq!(shown("windows"), ["mgba", "snes9x"]);
    }

    #[test]
    fn names_resolve_only_for_the_apps_of_this_os() {
        assert!(lookup_app("mgba").is_ok());
        assert_eq!(lookup_app("octomac").is_ok(), cfg!(target_os = "macos"));
    }
}
