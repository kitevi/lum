use std::path::{Path, PathBuf};

use anyhow::Result;

use super::catalog::{Requirement, VsixSpec};

impl Requirement {
    pub(crate) fn label(&self) -> String {
        match self {
            Requirement::Java { min_version, .. } => format!("Java {min_version}+"),
        }
    }

    pub(crate) fn locate(&self) -> Option<PathBuf> {
        match self {
            Requirement::Java { home_vars, .. } => find_java(home_vars),
        }
    }

    fn searched(&self) -> String {
        match self {
            Requirement::Java { home_vars, .. } => home_vars
                .iter()
                .map(|var| format!("${var}"))
                .chain(["PATH".to_owned()])
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

/// Fail before any editor or network work when a requirement is missing.
pub(crate) fn ensure(spec: &VsixSpec) -> Result<()> {
    match missing(spec) {
        Some(reason) => anyhow::bail!("{reason}"),
        None => Ok(()),
    }
}

/// Describe the first missing requirement, if any.
pub(crate) fn missing(spec: &VsixSpec) -> Option<String> {
    spec.requires
        .iter()
        .find(|requirement| requirement.locate().is_none())
        .map(|requirement| {
            format!(
                "{} requires {}, but none was found (checked {})",
                spec.name,
                requirement.label(),
                requirement.searched()
            )
        })
}

fn find_java(home_vars: &[&str]) -> Option<PathBuf> {
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    for var in home_vars {
        if let Some(home) = std::env::var_os(var).filter(|value| !value.is_empty()) {
            let candidate = PathBuf::from(home).join("bin").join(exe);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    which::which("java")
        .ok()
        .filter(|path| !is_macos_java_stub(path))
}

/// macOS ships `/usr/bin/java` even without a JDK; it only prompts to install one.
#[cfg(target_os = "macos")]
fn is_macos_java_stub(path: &Path) -> bool {
    use std::process::{Command, Stdio};

    path == Path::new("/usr/bin/java")
        && !Command::new("/usr/libexec/java_home")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
}

#[cfg(not(target_os = "macos"))]
fn is_macos_java_stub(_path: &Path) -> bool {
    false
}
