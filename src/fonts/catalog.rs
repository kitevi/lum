use anyhow::Result;

pub(crate) struct FontSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub download_url: &'static str,
    pub dir_name: &'static str,
    pub excluded_files: &'static [&'static str],
}

pub(crate) const CATALOG: &[FontSpec] = &[FontSpec {
    name: "dmca-sans-serif",
    description: "General-purpose sans serif font metric-compatible with Consolas",
    download_url: "https://typedesign.replit.app/DMCAsansserif9.0-20252.zip",
    dir_name: "dmca-sans-serif",
    // The upstream archive includes Arial/Tahoma alias files. Installing those
    // makes fontconfig prefer DMCA's metric-compatible aliases over the user's
    // real Windows fonts, and they advertise monospace spacing. Keep only the
    // DMCA family files.
    excluded_files: &[
        "Arial.ttf",
        "ArialBold.ttf",
        "ArialBoldItalic.ttf",
        "ArialItalic.ttf",
        "Tahoma.ttf",
        "TahomaBold.ttf",
        "TahomaBoldItalic.ttf",
        "TahomaItalic.ttf",
    ],
}];

pub(crate) fn lookup_font(name: &str) -> Result<&'static FontSpec> {
    CATALOG
        .iter()
        .find(|f| f.name == name)
        .ok_or_else(|| crate::cli::unknown_name("font", name, CATALOG.iter().map(|f| f.name)))
}
