use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use ignore::WalkBuilder;
use lzma_rust2::XzReader;

use super::catalog::ToolSpec;
use super::platform::{ArchiveType, Artifact};
use super::state::{ArtifactState, ToolState, bin_dir, load_state, save_state, tool_path};
use crate::artifact;

pub(crate) fn install_artifact(
    spec: &ToolSpec,
    artifact: &Artifact,
    preserve_installed_at: Option<SystemTime>,
) -> Result<ToolState> {
    fs::create_dir_all(bin_dir()?)?;
    let target = tool_path(spec)?;
    let temp_dir = tempfile::Builder::new().prefix("lum-tools-").tempdir()?;
    let download = temp_dir.path().join(&artifact.asset_name);
    crate::github::download_asset(&artifact.download_url, &download)?;
    let mut stored = load_state()?;
    let downloaded_sha256 = crate::github::sha256_file(&download)?;
    let pinned = stored
        .tools
        .get(spec.name)
        .filter(|previous| {
            previous.artifact.release_tag == artifact.release_tag
                && previous.artifact.asset_name == artifact.asset_name
        })
        .and_then(|previous| previous.artifact.checksum.as_deref());
    let expected = artifact
        .checksum_sha256
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or(pinned);
    crate::github::verify_checksum(
        expected,
        &downloaded_sha256,
        &format!(
            "{} release {} asset {}",
            spec.name, artifact.release_tag, artifact.asset_name
        ),
    )?;
    let source = materialize_binary(&download, temp_dir.path(), artifact)?;
    artifact::install_executable(&source, &target)?;

    let now = SystemTime::now();
    let installed_at = preserve_installed_at
        .or_else(|| stored.tools.get(spec.name).map(|s| s.installed_at))
        .unwrap_or(now);
    let tool_state = ToolState {
        installed: true,
        path: target,
        installed_version: artifact.version.clone(),
        installed_at,
        updated_at: now,
        artifact: ArtifactState {
            release_tag: artifact.release_tag.clone(),
            asset_name: artifact.asset_name.clone(),
            download_url: artifact.download_url.clone(),
            checksum: Some(downloaded_sha256),
        },
    };
    stored.tools.insert(spec.name.to_owned(), tool_state);
    save_state(&stored)?;
    Ok(stored.tools.remove(spec.name).unwrap())
}

fn materialize_binary(download: &Path, temp_dir: &Path, artifact: &Artifact) -> Result<PathBuf> {
    if artifact.archive_type == ArchiveType::Binary {
        return Ok(download.to_owned());
    }
    let extract_dir = temp_dir.join("extract");
    fs::create_dir_all(&extract_dir)?;
    extract_archive(download, &extract_dir, artifact.archive_type)?;
    let expected = extract_dir.join(Path::new(&artifact.binary_path));
    if expected.is_file() {
        return Ok(expected);
    }
    let basename = Path::new(&artifact.binary_path)
        .file_name()
        .context("artifact binary path missing filename")?;
    find_file_named(&extract_dir, basename).with_context(|| {
        format!(
            "could not find {} in extracted archive",
            artifact.binary_path
        )
    })
}

fn extract_archive(src: &Path, dest: &Path, archive_type: ArchiveType) -> Result<()> {
    match archive_type {
        ArchiveType::Zip => {
            let file = fs::File::open(src)?;
            zip::ZipArchive::new(file)?.extract(dest)?;
        }
        ArchiveType::TarGz => {
            let file = fs::File::open(src)?;
            tar::Archive::new(GzDecoder::new(file)).unpack(dest)?;
        }
        ArchiveType::TarXz => {
            let file = fs::File::open(src)?;
            tar::Archive::new(XzReader::new(file, false)).unpack(dest)?;
        }
        ArchiveType::Binary => {}
    }
    Ok(())
}

fn find_file_named(dir: &Path, filename: &std::ffi::OsStr) -> Result<PathBuf> {
    WalkBuilder::new(dir)
        .standard_filters(false)
        .follow_links(true)
        .build()
        .filter_map(Result::ok)
        .find(|entry| {
            entry.file_type().is_some_and(|ft| ft.is_file()) && entry.file_name() == filename
        })
        .map(|entry| entry.into_path())
        .context("not found")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialize_tar_xz_finds_nested_ctags() {
        let dir = tempfile::tempdir().unwrap();
        let download = dir.path().join("executables.tar.xz");
        fs::write(
            &download,
            include_bytes!("../../tests/fixtures/executables.tar.xz"),
        )
        .unwrap();
        let artifact = Artifact {
            version: "2026.10.08".into(),
            release_tag: "2026.10.08".into(),
            asset_name: "executables.tar.xz".into(),
            download_url: String::new(),
            archive_type: ArchiveType::TarXz,
            binary_path: "bin/ctags".into(),
            checksum_sha256: None,
        };

        let source = materialize_binary(&download, dir.path(), &artifact).unwrap();

        assert_eq!(fs::read(source).unwrap(), b"fixture ctags\n");
    }

    #[test]
    fn extract_tar_xz_rejects_corrupt_header() {
        let dir = tempfile::tempdir().unwrap();
        let download = dir.path().join("corrupt.tar.xz");
        let destination = dir.path().join("extract");
        fs::create_dir(&destination).unwrap();
        let mut archive = include_bytes!("../../tests/fixtures/executables.tar.xz").to_vec();
        archive[0] ^= 0xff;
        fs::write(&download, archive).unwrap();

        assert!(extract_archive(&download, &destination, ArchiveType::TarXz).is_err());
        assert_eq!(fs::read_dir(destination).unwrap().count(), 0);
    }

    #[test]
    fn materialize_finds_binary_in_hidden_ignored_directory() {
        let dir = tempfile::tempdir().unwrap();
        let download = dir.path().join("release.tar.gz");
        let file = fs::File::create(&download).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        for (path, contents) in [
            ("bundle/.ignore", &b"nested/\n"[..]),
            ("bundle/nested/.bin/scc", &b"binary"[..]),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            archive.append_data(&mut header, path, contents).unwrap();
        }
        archive.into_inner().unwrap().finish().unwrap();
        let artifact = Artifact {
            version: "1.0.0".into(),
            release_tag: "v1.0.0".into(),
            asset_name: "release.tar.gz".into(),
            download_url: String::new(),
            archive_type: ArchiveType::TarGz,
            binary_path: "scc".into(),
            checksum_sha256: None,
        };

        let source = materialize_binary(&download, dir.path(), &artifact).unwrap();

        assert_eq!(fs::read(source).unwrap(), b"binary");
    }
}
