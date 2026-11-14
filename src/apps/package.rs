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

/// How a bundle or program folder is packed, from its asset's file name.
enum Archive {
    DiskImage,
    Zip,
    SevenZip,
}

impl Archive {
    fn of(asset: &Path) -> Result<Self> {
        let name = asset
            .file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        Ok(if name.ends_with(".dmg") {
            Self::DiskImage
        } else if name.ends_with(".zip") {
            Self::Zip
        } else if name.ends_with(".7z") {
            Self::SevenZip
        } else {
            bail!("{name} is not a disk image, zip or 7z archive")
        })
    }
}

/// Unpack `asset` inside `scratch` and return the app folder (or macOS bundle),
/// ready to move into place.
pub(crate) fn unpack(package: &Package, asset: &Path, scratch: &Path) -> Result<PathBuf> {
    let unpacked = match package {
        Package::AppImage => {
            let app = scratch.join("app");
            unpack_appimage(asset, &app).map(|()| app)
        }
        Package::Bundle { bundle } => unpack_bundle(asset, bundle, scratch),
        Package::Folder {
            program, remove, ..
        } => unpack_folder(asset, program, remove, scratch),
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

/// A macOS bundle, copied out of a disk image or extracted from an archive that
/// holds it at its root.
fn unpack_bundle(asset: &Path, bundle: &str, scratch: &Path) -> Result<PathBuf> {
    if let Archive::DiskImage = Archive::of(asset)? {
        let app = scratch.join(bundle);
        copy_from_disk_image(asset, bundle, &app)?;
        return Ok(app);
    }
    let extracted = scratch.join("extracted");
    extract(asset, &extracted)?;
    let app = extracted.join(bundle);
    if !app.is_dir() {
        bail!("{bundle} is not in the archive");
    }
    Ok(app)
}

/// A program folder: the archive's only folder, or its root when it holds the
/// program's files directly. The files in `remove` are dropped.
fn unpack_folder(asset: &Path, program: &str, remove: &[&str], scratch: &Path) -> Result<PathBuf> {
    let extracted = scratch.join("extracted");
    extract(asset, &extracted)?;
    let entries = fs::read_dir(&extracted)?.collect::<io::Result<Vec<_>>>()?;
    let app = match entries.as_slice() {
        [entry] if entry.file_type()?.is_dir() => entry.path(),
        _ => extracted,
    };
    for name in remove {
        super::remove_file_if_present(&app.join(name))?;
    }
    if !app.join(program).is_file() {
        bail!("{program} is not in the archive");
    }
    Ok(app)
}

/// Extract a zip or 7z archive. Both crates refuse entries that would land
/// outside `dest`, and zip restores the symlinks and modes of a bundle.
fn extract(archive: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    match Archive::of(archive)? {
        Archive::Zip => zip::ZipArchive::new(fs::File::open(archive)?)?.extract(dest)?,
        Archive::SevenZip => sevenz_rust2::decompress_file(archive, dest)?,
        Archive::DiskImage => bail!("a disk image holds a macOS bundle, not a program folder"),
    }
    Ok(())
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

    fn folder(program: &'static str, remove: &'static [&'static str]) -> Package {
        Package::Folder {
            program,
            remove,
            keep: &[],
        }
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

        let package = folder_package_mgba();
        let app = unpack(&package, &archive, &dir.path().join("scratch")).unwrap();

        assert_eq!(app.file_name().unwrap(), "mGBA-1.0-win64");
        assert_eq!(fs::read_to_string(app.join("mGBA.exe")).unwrap(), "exe");
        assert!(!app.join("portable.ini").exists());
    }

    fn folder_package_mgba() -> Package {
        folder("mGBA.exe", &["portable.ini"])
    }

    #[test]
    fn unpacks_a_flat_zip_as_the_program_folder() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("snes9x-1.0-win32-x64.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        for (name, contents) in [("snes9x-x64.exe", "exe"), ("readme-windows.txt", "docs")] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            io::Write::write_all(&mut zip, contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap();

        let app = unpack(
            &folder("snes9x-x64.exe", &[]),
            &archive,
            &dir.path().join("scratch"),
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(app.join("snes9x-x64.exe")).unwrap(),
            "exe"
        );
        assert!(app.join("readme-windows.txt").exists());
    }

    #[test]
    fn a_folder_without_its_program_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("other.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        zip.start_file("other.exe", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.finish().unwrap();

        let error = unpack(
            &folder("snes9x-x64.exe", &[]),
            &archive,
            &dir.path().join("scratch"),
        )
        .unwrap_err();

        assert!(format!("{error:#}").contains("snes9x-x64.exe is not in the archive"));
    }

    #[cfg(unix)]
    #[test]
    fn unpacks_a_zipped_bundle_with_its_symlinks_and_modes() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("Snes9x-1.0-Mac.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        let executable = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        zip.start_file("Snes9x.app/Contents/MacOS/Snes9x", executable)
            .unwrap();
        io::Write::write_all(&mut zip, b"binary").unwrap();
        zip.add_directory(
            "Snes9x.app/Contents/Frameworks/core.framework/Versions/A",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.add_symlink(
            "Snes9x.app/Contents/Frameworks/core.framework/Versions/Current",
            "A",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.finish().unwrap();

        let package = Package::Bundle {
            bundle: "Snes9x.app",
        };
        let app = unpack(&package, &archive, &dir.path().join("scratch")).unwrap();

        assert_eq!(app.file_name().unwrap(), "Snes9x.app");
        let program = app.join("Contents/MacOS/Snes9x");
        assert_eq!(
            fs::metadata(&program).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(
            fs::read_link(app.join("Contents/Frameworks/core.framework/Versions/Current")).unwrap(),
            Path::new("A")
        );
    }

    #[test]
    fn unpacks_a_seven_zip_without_a_single_folder_as_its_root() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("a.exe"), "a").unwrap();
        fs::write(source.join("b.dll"), "b").unwrap();
        let archive = dir.path().join("loose.7z");
        sevenz_rust2::compress_to_path(&source, &archive).unwrap();

        let app = unpack(&folder("a.exe", &[]), &archive, &dir.path().join("scratch")).unwrap();

        assert!(app.join("a.exe").is_file());
        assert!(app.join("b.dll").is_file());
    }

    #[test]
    fn assets_are_unpacked_by_their_extension() {
        assert!(matches!(
            Archive::of(Path::new("Snes9x-1.63-Mac.zip")).unwrap(),
            Archive::Zip
        ));
        assert!(matches!(
            Archive::of(Path::new("mGBA-0.10.5-macos.dmg")).unwrap(),
            Archive::DiskImage
        ));
        assert!(matches!(
            Archive::of(Path::new("mGBA-0.10.5-win64.7z")).unwrap(),
            Archive::SevenZip
        ));
        assert!(Archive::of(Path::new("setup.exe")).is_err());
    }
}
