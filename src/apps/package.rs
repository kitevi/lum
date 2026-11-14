//! Unpacking release assets into an app folder.

use std::{
    fs,
    io::{self, BufReader, Read},
    path::{Component, Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use backhand::{FilesystemReader, InnerNode};

use super::catalog::Package;

/// Unpack `asset` inside `scratch` and return the app folder (or macOS bundle),
/// ready to move into place.
pub(crate) fn unpack(package: &Package, asset: &Path, scratch: &Path) -> Result<PathBuf> {
    let unpacked = match package {
        Package::AppImage => {
            let app = scratch.join("app");
            unpack_appimage(asset, &app).map(|()| app)
        }
        Package::DiskImage { bundle } => {
            let app = scratch.join(bundle);
            copy_from_disk_image(asset, bundle, &app).map(|()| app)
        }
        Package::SevenZip { remove, .. } => {
            unpack_seven_zip(asset, &scratch.join("extracted"), remove)
        }
    };
    unpacked.with_context(|| format!("failed to unpack {}", asset.display()))
}

/// Unpack the SquashFS image an AppImage carries after its ELF runtime. The app
/// then runs without FUSE, and lum never executes the runtime.
fn unpack_appimage(image: &Path, dest: &Path) -> Result<()> {
    let mut file = fs::File::open(image)?;
    let offset = elf_size(&mut file).context("not an AppImage")?;
    let filesystem = FilesystemReader::from_reader_with_offset(BufReader::new(file), offset)?;
    for node in filesystem.files() {
        let path = inside(dest, &node.fullpath)?;
        match &node.inner {
            InnerNode::Dir(_) => fs::create_dir_all(&path)?,
            InnerNode::File(file) => {
                create_parent(&path)?;
                let mut out = fs::File::create(&path)?;
                io::copy(&mut filesystem.file(file).reader(), &mut out)?;
                set_mode(&out, node.header.permissions)?;
            }
            InnerNode::Symlink(link) => {
                create_parent(&path)?;
                symlink(&link.link, &path)?;
            }
            // Devices, pipes and sockets have no place in an app.
            _ => {}
        }
    }
    Ok(())
}

/// Length of the 64-bit little-endian ELF runtime at the start of an AppImage:
/// its section header table ends the ELF file, and the image follows.
fn elf_size(file: &mut impl Read) -> Result<u64> {
    let mut header = [0u8; 64];
    file.read_exact(&mut header)?;
    if header[..4] != *b"\x7fELF" || header[4] != 2 || header[5] != 1 {
        bail!("expected a 64-bit little-endian ELF runtime");
    }
    let table = u64::from_le_bytes(header[0x28..0x30].try_into()?);
    let entry_size = u16::from_le_bytes(header[0x3a..0x3c].try_into()?);
    let entries = u16::from_le_bytes(header[0x3c..0x3e].try_into()?);
    table
        .checked_add(u64::from(entry_size) * u64::from(entries))
        .context("corrupt ELF header")
}

/// `entry`, an absolute path inside the image, below `dest`. Refuses paths that
/// leave `dest` or pass through a symlink the image created.
fn inside(dest: &Path, entry: &Path) -> Result<PathBuf> {
    let mut path = dest.to_path_buf();
    for component in entry.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(part) => {
                if fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()) {
                    bail!("{} passes through a symlink", entry.display());
                }
                path.push(part);
            }
            Component::ParentDir | Component::Prefix(_) => {
                bail!("{} leaves the app folder", entry.display());
            }
        }
    }
    Ok(path)
}

fn create_parent(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(parent) => fs::create_dir_all(parent),
        None => Ok(()),
    }
}

#[cfg(unix)]
fn set_mode(file: &fs::File, mode: u16) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(u32::from(mode) & 0o777))
}

#[cfg(not(unix))]
fn set_mode(_: &fs::File, _: u16) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn symlink(_: &Path, link: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!("cannot create symlink {}", link.display()),
    ))
}

/// Copy `bundle` out of a disk image with the system's hdiutil and ditto: no
/// maintained Rust crate reads HFS+, and ditto keeps a bundle's symlinks,
/// permissions and extended attributes intact.
fn copy_from_disk_image(image: &Path, bundle: &str, dest: &Path) -> Result<()> {
    let mount = tempfile::Builder::new().prefix("lum-dmg-").tempdir()?;
    super::run_tool(
        Command::new("hdiutil")
            .args([
                "attach",
                "-nobrowse",
                "-noautoopen",
                "-readonly",
                "-mountpoint",
            ])
            .arg(mount.path())
            .arg(image),
    )?;
    let source = mount.path().join(bundle);
    let copied = if source.is_dir() {
        super::run_tool(Command::new("ditto").arg(&source).arg(dest))
    } else {
        Err(anyhow::anyhow!("{bundle} is not in the disk image"))
    };
    let detach = |force: bool| {
        let mut command = Command::new("hdiutil");
        command.args(["detach", "-quiet"]);
        if force {
            command.arg("-force");
        }
        super::run_tool(command.arg(mount.path()))
    };
    // A volume Spotlight is still indexing refuses a plain detach.
    if let Err(error) = detach(false).or_else(|_| detach(true)) {
        tracing::warn!(error = %error, mount = %mount.path().display(), "failed to detach disk image");
    }
    copied
}

/// Unpack a 7z archive holding one top-level folder, minus the files in `remove`.
fn unpack_seven_zip(archive: &Path, dest: &Path, remove: &[&str]) -> Result<PathBuf> {
    sevenz_rust2::decompress_file(archive, dest)?;
    let entries = fs::read_dir(dest)?.collect::<io::Result<Vec<_>>>()?;
    let [entry] = entries.as_slice() else {
        bail!(
            "expected one top-level folder, found {} entries",
            entries.len()
        );
    };
    if !entry.file_type()?.is_dir() {
        bail!(
            "expected one top-level folder, found {}",
            entry.path().display()
        );
    }
    let app = entry.path();
    for name in remove {
        super::remove_file_if_present(&app.join(name))?;
    }
    Ok(app)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elf_header(table: u64, entry_size: u16, entries: u16) -> Vec<u8> {
        let mut header = vec![0u8; 64];
        header[..6].copy_from_slice(b"\x7fELF\x02\x01");
        header[0x28..0x30].copy_from_slice(&table.to_le_bytes());
        header[0x3a..0x3c].copy_from_slice(&entry_size.to_le_bytes());
        header[0x3c..0x3e].copy_from_slice(&entries.to_le_bytes());
        header
    }

    #[test]
    fn elf_size_ends_at_the_section_header_table() {
        // The layout of mGBA 0.10.5's x86_64 AppImage runtime.
        let header = elf_header(192_384, 64, 21);
        assert_eq!(elf_size(&mut header.as_slice()).unwrap(), 193_728);
    }

    #[test]
    fn elf_size_rejects_other_files() {
        let mut header = elf_header(0, 0, 0);
        header[4] = 1;
        assert!(elf_size(&mut header.as_slice()).is_err());
        assert!(elf_size(&mut [0u8; 64].as_slice()).is_err());
    }

    #[test]
    fn inside_keeps_entries_below_the_app_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            inside(dir.path(), Path::new("/usr/bin/app")).unwrap(),
            dir.path().join("usr/bin/app")
        );
        assert_eq!(inside(dir.path(), Path::new("/")).unwrap(), dir.path());
        assert!(inside(dir.path(), Path::new("/usr/../../escape")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn inside_refuses_paths_through_a_symlink() {
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/tmp", dir.path().join("link")).unwrap();
        assert!(inside(dir.path(), Path::new("/link")).is_ok());
        assert!(inside(dir.path(), Path::new("/link/file")).is_err());
    }

    #[test]
    fn unpacks_the_seven_zip_folder_without_portable_ini() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("source").join("mGBA-1.0-win64");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("mGBA.exe"), "exe").unwrap();
        fs::write(folder.join("portable.ini"), "").unwrap();
        let archive = dir.path().join("mgba.7z");
        sevenz_rust2::compress_to_path(dir.path().join("source"), &archive).unwrap();

        let package = Package::SevenZip {
            program: "mGBA.exe",
            remove: &["portable.ini"],
        };
        let app = unpack(&package, &archive, &dir.path().join("scratch")).unwrap();

        assert_eq!(app.file_name().unwrap(), "mGBA-1.0-win64");
        assert_eq!(fs::read_to_string(app.join("mGBA.exe")).unwrap(), "exe");
        assert!(!app.join("portable.ini").exists());
    }

    #[test]
    fn seven_zip_without_a_single_folder_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("a.exe"), "a").unwrap();
        fs::write(source.join("b.dll"), "b").unwrap();
        let archive = dir.path().join("loose.7z");
        sevenz_rust2::compress_to_path(&source, &archive).unwrap();

        let package = Package::SevenZip {
            program: "a.exe",
            remove: &[],
        };
        let error = unpack(&package, &archive, &dir.path().join("scratch")).unwrap_err();
        assert!(format!("{error:#}").contains("expected one top-level folder"));
    }
}
