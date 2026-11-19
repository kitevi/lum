use anyhow::{Result, bail};
use std::path::PathBuf;

pub(crate) const KEEP_BACKUPS: usize = 3;

pub(crate) const GLOBAL_EXCLUDE_GLOBS: &[&str] = &[
    "*.DS_Store",
    "._*",
    "Thumbs.db",
    "desktop.ini",
    "*.swp",
    "*.swo",
    "*~",
    ".Spotlight-V100",
    ".Trashes",
    ".fseventsd",
    ".TemporaryItems",
    ".git",
    ".svn",
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct BackupTarget {
    pub(crate) name: &'static str,
    pub(crate) path: &'static str,
    pub(crate) allowed_os: &'static [&'static str],
    pub(crate) backup_prefix: &'static str,
    /// Command to run before archiving, found on PATH. Its job is to refresh
    /// files inside the target that something else owns.
    pub(crate) pre_command: Option<&'static str>,
}

pub(crate) const BIO: BackupTarget = BackupTarget {
    name: "bio",
    path: ".bio",
    allowed_os: &["macos", "linux"],
    backup_prefix: ".bio.backup-",
    // ~/.bio/install/Brewfile only changes when brdump runs, so archiving
    // without it would upload a package list from some earlier machine state.
    pre_command: Some("brdump"),
};

/// ROMs, with the saves and save states mGBA keeps beside each ROM.
pub(crate) const ROMS: BackupTarget = BackupTarget {
    name: "roms",
    path: crate::paths::ROMS_FOLDER,
    allowed_os: &["macos", "linux", "windows"],
    backup_prefix: ".roms.backup-",
    // Emulators write saves as they go; nothing outside needs to run first.
    pre_command: None,
};

impl BackupTarget {
    pub(crate) fn ensure_current_os_allowed(self) -> Result<()> {
        let current = std::env::consts::OS;
        if self.allowed_os.contains(&current) {
            return Ok(());
        }

        bail!(
            "{} backup is only supported on {:?} (current OS: {})",
            self.name,
            self.allowed_os,
            current
        );
    }

    pub(crate) fn target_path(self, home: &std::path::Path) -> PathBuf {
        home.join(self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_refuse_other_operating_systems() {
        let target = BackupTarget {
            allowed_os: &["plan9"],
            ..BIO
        };

        let error = target.ensure_current_os_allowed().unwrap_err();

        assert!(
            error
                .to_string()
                .contains(r#"bio backup is only supported on ["plan9"]"#)
        );
    }
}
