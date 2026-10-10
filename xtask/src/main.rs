use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("install") => match args.next().as_deref() {
            None => install(),
            Some("-h" | "--help") => {
                print_help();
                Ok(())
            }
            Some(_) => bail!("usage: cargo local-install"),
        },
        Some("-h" | "--help") | None => {
            print_help();
            Ok(())
        }
        Some(command) => bail!("unknown xtask command: {command}"),
    }
}

fn print_help() {
    println!("xtask commands:");
    println!("  install    build lum in release mode and install it to a user bin directory");
}

fn install() -> Result<()> {
    let repo = repo_root()?;
    run_command(
        Command::new("cargo")
            .arg("build")
            .arg("--release")
            .current_dir(&repo),
    )?;

    let binary = repo.join("target").join("release").join(binary_name("lum"));
    let install_dir = choose_install_dir()?;
    fs::create_dir_all(&install_dir).with_context(|| {
        format!(
            "failed to create install directory {}",
            install_dir.display()
        )
    })?;

    let dst = install_dir.join(binary_name("lum"));
    install_binary(&binary, &dst)?;
    println!("Installed to {}", dst.display());
    Ok(())
}

fn repo_root() -> Result<PathBuf> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(PathBuf::from)
        .context("failed to determine repository root")
}

fn choose_install_dir() -> Result<PathBuf> {
    let home = home_dir()?;
    if let Ok(prefix) = env::var("PREFIX") {
        let prefix = PathBuf::from(prefix);
        ensure_prefix_allowed(&home, &prefix)?;
        let dir = prefix.join("bin");
        if !path_contains_dir(&dir) {
            bail!("{} is not in PATH", dir.display());
        }
        return Ok(dir);
    }

    for candidate in preferred_dirs(&home) {
        if path_contains_dir(&candidate) {
            return Ok(candidate);
        }
    }

    for fallback in fallback_dirs(&home) {
        if fallback.is_dir() {
            eprintln!(
                "warning: could not find supported install directory in PATH; falling back to {}",
                fallback.display()
            );
            eprintln!("warning: installed command may not be directly runnable");
            return Ok(fallback);
        }
    }

    bail!("no suitable install directory found")
}

fn preferred_dirs(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".bio").join("bin"),
        home.join(".local").join("bin"),
        home.join("bin"),
    ]
}

fn fallback_dirs(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join("Desktop"),
        home.join("Downloads"),
        home.to_path_buf(),
    ]
}

fn home_dir() -> Result<PathBuf> {
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .context("HOME/USERPROFILE is not set")?;
    if !home.is_absolute() {
        bail!("home directory must be absolute: {}", home.display());
    }
    Ok(home)
}

fn ensure_prefix_allowed(home: &Path, prefix: &Path) -> Result<()> {
    if !prefix.is_absolute() {
        bail!("PREFIX must be an absolute path");
    }
    if prefix == Path::new(std::path::MAIN_SEPARATOR_STR) {
        bail!("PREFIX must not be filesystem root");
    }
    if !prefix.starts_with(home) {
        bail!("PREFIX must be inside home directory");
    }
    Ok(())
}

fn path_contains_dir(dir: &Path) -> bool {
    env::var_os("PATH")
        .map(|path| {
            env::split_paths(&path).any(|entry| same_path_without_trailing_slash(&entry, dir))
        })
        .unwrap_or(false)
}

fn same_path_without_trailing_slash(left: &Path, right: &Path) -> bool {
    normalize_trailing_slash(left) == normalize_trailing_slash(right)
}

fn normalize_trailing_slash(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().trim_end_matches(['/', '\\']))
}

fn install_binary(src: &Path, dst: &Path) -> Result<()> {
    if !src.is_file() {
        bail!("built binary is missing: {}", src.display());
    }
    let temp = tempfile::NamedTempFile::new_in(dst.parent().unwrap_or(Path::new(".")))
        .with_context(|| format!("failed to stage {}", dst.display()))?;
    fs::copy(src, temp.path()).with_context(|| {
        format!(
            "failed to copy {} to {}",
            src.display(),
            temp.path().display()
        )
    })?;
    set_executable(temp.path())?;
    temp.persist(dst)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to install {}", dst.display()))?;
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn binary_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn run_command(command: &mut Command) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("failed to start command: {command:?}"))?;
    if !status.success() {
        bail!("command failed with {status}: {command:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_dirs_match_installer_policy() {
        let home = PathBuf::from(if cfg!(windows) {
            r"C:\Users\pun"
        } else {
            "/home/pun"
        });
        let dirs = preferred_dirs(&home);
        assert_eq!(dirs[0], home.join(".bio").join("bin"));
        assert_eq!(dirs[1], home.join(".local").join("bin"));
        assert_eq!(dirs[2], home.join("bin"));
    }

    #[test]
    fn fallback_dirs_match_installer_policy() {
        let home = Path::new("home");
        assert_eq!(
            fallback_dirs(home),
            [
                home.join("Desktop"),
                home.join("Downloads"),
                home.to_path_buf()
            ]
        );
    }

    #[test]
    fn prefix_must_be_inside_home() {
        let home = PathBuf::from(if cfg!(windows) {
            r"C:\Users\pun"
        } else {
            "/home/pun"
        });
        assert!(ensure_prefix_allowed(&home, &home.join(".local")).is_ok());
        let outside = if cfg!(windows) {
            PathBuf::from(r"C:\other")
        } else {
            PathBuf::from("/opt")
        };
        assert!(ensure_prefix_allowed(&home, &outside).is_err());
    }

    #[test]
    fn path_comparison_ignores_trailing_slashes() {
        let base = if cfg!(windows) {
            PathBuf::from(r"C:\Users\pun\.local\bin")
        } else {
            PathBuf::from("/home/pun/.local/bin")
        };
        let with_slash = PathBuf::from(format!("{}{}", base.display(), std::path::MAIN_SEPARATOR));
        assert!(same_path_without_trailing_slash(&base, &with_slash));
    }

    #[test]
    fn binary_name_uses_windows_extension_only_on_windows() {
        let expected = if cfg!(windows) { "lum.exe" } else { "lum" };
        assert_eq!(binary_name("lum"), expected);
    }

    #[test]
    fn failed_install_preserves_target_and_cleans_staging() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("lum");
        fs::write(&source, b"new").unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), b"old").unwrap();

        let result = install_binary(&source, &target);

        assert!(result.is_err());
        assert_eq!(fs::read(target.join("keep")).unwrap(), b"old");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn install_replaces_existing_executable() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("lum");
        fs::write(&source, b"new").unwrap();
        fs::write(&target, b"old").unwrap();

        install_binary(&source, &target).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(target).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
    }
}
