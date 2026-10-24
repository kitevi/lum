use std::fs;
use std::io;
use std::path::Path;

use anyhow::{Context, Result};

use super::catalog::FontSpec;
use super::paths;

pub(crate) fn install_font(spec: &FontSpec) -> Result<()> {
    let font_dir = paths::font_install_dir(spec)?;
    let fonts_root = font_dir
        .parent()
        .context("font install directory has no parent")?;
    fs::create_dir_all(fonts_root)?;

    // Stage the download and extracted files beside the final destination so a
    // failed download/extract does not destroy an existing installation.
    let temp_dir = tempfile::Builder::new()
        .prefix(".lum-font-")
        .tempdir_in(fonts_root)?;
    let zip_path = temp_dir.path().join("font.zip");
    let staging_dir = temp_dir.path().join("staging");
    fs::create_dir_all(&staging_dir)?;

    let url = test_artifact_url(spec).unwrap_or_else(|| spec.download_url.to_owned());
    crate::github::download_asset(&url, &zip_path)?;
    extract_ttf_files(spec, &zip_path, &staging_dir)?;

    if font_dir.exists() {
        fs::remove_dir_all(&font_dir)?;
    }
    fs::rename(&staging_dir, &font_dir)?;

    // Refresh font cache (best effort)
    paths::refresh_font_cache();

    Ok(())
}

fn test_artifact_url(spec: &FontSpec) -> Option<String> {
    let env_key = format!(
        "LUM_FONT_TEST_ARTIFACT_{}",
        spec.name.replace('-', "_").to_ascii_uppercase()
    );
    std::env::var(env_key).ok()
}

fn extract_ttf_files(spec: &FontSpec, zip_path: &Path, dest: &Path) -> Result<()> {
    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let mut installed = 0u32;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_owned();

        // Skip directories and non-TTF files
        if entry.is_dir() || !name.to_ascii_lowercase().ends_with(".ttf") {
            continue;
        }

        // Use only the filename, discard any directory prefix in the zip
        let file_name = Path::new(&name)
            .file_name()
            .context(format!("zip entry has no filename: {name}"))?;
        let file_name_str = file_name.to_string_lossy();

        if spec
            .excluded_files
            .iter()
            .any(|excluded| file_name_str.eq_ignore_ascii_case(excluded))
        {
            continue;
        }

        let out_path = dest.join(file_name);

        let mut out_file = fs::File::create(&out_path)?;
        io::copy(&mut entry, &mut out_file)?;
        installed += 1;
    }

    if installed == 0 {
        anyhow::bail!("no TTF font files found in archive");
    }

    Ok(())
}
