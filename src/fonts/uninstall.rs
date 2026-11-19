use std::fs;

use anyhow::Result;

use super::catalog::FontSpec;
use super::paths;

pub(crate) fn uninstall_font(spec: &FontSpec) -> Result<()> {
    let font_dir = paths::font_install_dir(spec)?;
    fs::remove_dir_all(&font_dir)?;

    // Best-effort font cache refresh
    paths::refresh_font_cache();

    Ok(())
}
