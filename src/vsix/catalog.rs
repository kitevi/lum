use anyhow::{Context, Result};

pub(crate) enum Requirement {
    /// A Java runtime: `<var>/bin/java` for each home variable in order, then `java` on PATH.
    /// Only presence is checked; `min_version` is reported, not enforced.
    Java {
        min_version: u32,
        home_vars: &'static [&'static str],
    },
}

pub(crate) struct VsixSpec {
    pub name: &'static str,
    pub extension_id: &'static str,
    pub description: &'static str,
    pub owner: &'static str,
    pub repo: &'static str,
    /// Release asset is named `<asset_prefix>-<version>.vsix`.
    pub asset_prefix: &'static str,
    pub requires: &'static [Requirement],
    /// Extensions that clash with this one; lum prints a note when they are installed.
    pub conflicts: &'static [&'static str],
}

pub(crate) const CATALOG: &[VsixSpec] = &[VsixSpec {
    name: "lathe",
    extension_id: "ag-libs.lathe",
    description: "Lathe for Java: language server derived from your build",
    owner: "ag-libs",
    repo: "lathe",
    asset_prefix: "lathe",
    requires: &[Requirement::Java {
        min_version: 17,
        home_vars: &["LATHE_JAVA_HOME", "JAVA_HOME"],
    }],
    conflicts: &["redhat.java"],
}];

impl VsixSpec {
    pub(crate) fn asset_name(&self, version: &str) -> String {
        format!("{}-{version}.vsix", self.asset_prefix)
    }

    pub(crate) fn version_from_asset<'a>(&self, asset_name: &'a str) -> Option<&'a str> {
        asset_name
            .strip_prefix(self.asset_prefix)?
            .strip_prefix('-')?
            .strip_suffix(".vsix")
            .filter(|version| !version.is_empty())
    }
}

pub(crate) fn test_artifact_env(spec: &VsixSpec) -> String {
    format!(
        "LUM_VSIX_TEST_ARTIFACT_{}",
        spec.name.replace('-', "_").to_ascii_uppercase()
    )
}

pub(crate) fn lookup_extension(name: &str) -> Result<&'static VsixSpec> {
    CATALOG
        .iter()
        .find(|spec| spec.name == name)
        .with_context(|| {
            format!(
                "unknown managed extension {name:?} (available: {})",
                available_extensions()
            )
        })
}

pub(crate) fn available_extensions() -> String {
    CATALOG
        .iter()
        .map(|spec| spec.name)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_name_round_trips_through_version_from_asset() {
        let spec = &CATALOG[0];
        assert_eq!(spec.asset_name("0.1.24"), "lathe-0.1.24.vsix");
        assert_eq!(spec.version_from_asset("lathe-0.1.24.vsix"), Some("0.1.24"));
    }

    #[test]
    fn version_from_asset_rejects_other_names() {
        let spec = &CATALOG[0];
        assert_eq!(spec.version_from_asset("lathe-.vsix"), None);
        assert_eq!(spec.version_from_asset("lathe-0.1.24.zip"), None);
        assert_eq!(spec.version_from_asset("other-0.1.24.vsix"), None);
        assert_eq!(spec.version_from_asset("lathe0.1.24.vsix"), None);
    }
}
