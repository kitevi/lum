use std::{io::Write, path::Path};

use anyhow::Result;
use tempfile::NamedTempFile;

pub(crate) fn install_executable(source: &Path, target: &Path) -> Result<()> {
    let mut temp = NamedTempFile::new_in(target.parent().unwrap_or(Path::new(".")))?;
    let mut source = std::fs::File::open(source)?;
    std::io::copy(&mut source, temp.as_file_mut())?;
    persist_executable(temp, target)
}

pub(crate) fn write_executable(target: &Path, bytes: &[u8]) -> Result<()> {
    let mut temp = NamedTempFile::new_in(target.parent().unwrap_or(Path::new(".")))?;
    temp.write_all(bytes)?;
    persist_executable(temp, target)
}

fn persist_executable(temp: NamedTempFile, target: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o755))?;
    }
    temp.persist(target).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_replaces_executable_without_touching_neighbor() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("tool");
        let neighbor = dir.path().join("tool.lum-tmp");
        std::fs::write(&target, b"old").unwrap();
        std::fs::write(&neighbor, b"unrelated").unwrap();

        write_executable(&target, b"new").unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert!(neighbor.exists(), "replacement removed a neighboring file");
        assert_eq!(std::fs::read(neighbor).unwrap(), b"unrelated");
    }

    #[test]
    fn install_replaces_existing_executable() {
        let source_dir = tempfile::tempdir().unwrap();
        let target_dir = tempfile::tempdir().unwrap();
        let source = source_dir.path().join("source");
        let target = target_dir.path().join("tool");
        std::fs::write(&source, b"new").unwrap();
        std::fs::write(&target, b"old").unwrap();

        install_executable(&source, &target).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert_eq!(std::fs::read_dir(target_dir.path()).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(target).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
    }

    #[test]
    fn failed_write_preserves_target_and_cleans_staging() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("tool");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep"), b"old").unwrap();

        let result = write_executable(&target, b"new");

        assert!(result.is_err());
        assert_eq!(std::fs::read(target.join("keep")).unwrap(), b"old");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
