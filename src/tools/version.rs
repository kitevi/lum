use std::{cmp::Ordering, path::Path, process::Command};

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use semver::Version;

use super::catalog::ToolSpec;

pub(crate) fn probe_version(spec: &ToolSpec, path: &Path) -> Result<String> {
    if spec.version_args.is_empty() {
        bail!("no version args configured");
    }
    let output = Command::new(path).args(spec.version_args).output()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if text.is_empty() {
        bail!("version command produced no output");
    }
    Ok(extract_version(&text).unwrap_or(text))
}

fn extract_version(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|part| part.chars().any(|c| c.is_ascii_digit()))
        .map(|s| {
            let s = s.trim_start_matches('v');
            // Strip any leading non-numeric prefix (e.g. "tool-" from "tool-1.8.1").
            if let Some(pos) = s.find(|c: char| c.is_ascii_digit()) {
                // Keep any delimiter right before the digit (e.g. keep the '-' in
                // "v1.8.1" is already stripped, but for "tool-1.8.1" we drop
                // "tool-" so compare_versions sees "1.8.1").
                let prefix_end = s[..pos].rfind(['-', '.']).map_or(pos, |d| d + 1);
                s[prefix_end..].to_owned()
            } else {
                s.to_owned()
            }
        })
}

pub(crate) fn compare_versions(spec: &ToolSpec, a: &str, b: &str) -> Result<Ordering> {
    if a == b {
        return Ok(Ordering::Equal);
    }
    if spec.name == "universal-ctags" {
        let parse = |raw: &str| {
            let version = raw.trim().trim_start_matches('v');
            let date = version.split_once('+').map_or(version, |(date, _)| date);
            NaiveDate::parse_from_str(date, "%Y.%m.%d")
                .with_context(|| format!("invalid nightly version {raw:?}"))
        };
        return Ok(parse(a)?.cmp(&parse(b)?));
    }
    Ok(parse_version(a)?.cmp_precedence(&parse_version(b)?))
}

fn parse_version(raw: &str) -> Result<Version> {
    let version = raw.trim().trim_start_matches('v');
    let (core, suffix) = version
        .find(['-', '+'])
        .map_or((version, ""), |index| version.split_at(index));
    let normalized = match core.split('.').count() {
        1 => format!("{core}.0.0{suffix}"),
        2 => format!("{core}.0{suffix}"),
        _ => version.to_owned(),
    };
    Version::parse(&normalized).with_context(|| format!("invalid version {raw:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_tool_style_version() {
        assert_eq!(extract_version("tool-1.8.1"), Some("1.8.1".to_owned()));
    }

    #[test]
    fn extract_plain_version() {
        assert_eq!(extract_version("1.8.1"), Some("1.8.1".to_owned()));
    }

    #[test]
    fn extract_v_prefixed_version() {
        assert_eq!(extract_version("v1.8.1"), Some("1.8.1".to_owned()));
    }

    #[test]
    fn extract_version_from_sentence() {
        assert_eq!(
            extract_version("scc version 3.5.0"),
            Some("3.5.0".to_owned())
        );
    }

    fn compare_scc(a: &str, b: &str) -> Result<Ordering> {
        let spec = super::super::catalog::lookup_tool("scc").unwrap();
        compare_versions(spec, a, b)
    }

    #[test]
    fn compare_tool_runtime_vs_release() {
        assert_eq!(compare_scc("1.8.1", "1.8.1").unwrap(), Ordering::Equal);
    }

    #[test]
    fn compare_prerelease_precedes_stable() {
        assert_eq!(compare_scc("1.0.0-rc.1", "1.0.0").unwrap(), Ordering::Less);
    }

    #[test]
    fn compare_ignores_build_metadata() {
        assert_eq!(
            compare_scc("1.2.3+build.7", "1.2.3").unwrap(),
            Ordering::Equal
        );
    }

    #[test]
    fn compare_normalizes_prefixes_and_abbreviated_versions() {
        for version in ["v1", "1.0", "v1.0.0"] {
            assert_eq!(compare_scc(version, "1.0.0").unwrap(), Ordering::Equal);
        }
        assert_eq!(compare_scc("v1.2-rc.1", "1.2.0").unwrap(), Ordering::Less);
    }

    #[test]
    fn compare_ctags_nightly_dates() {
        let spec = super::super::catalog::lookup_tool("universal-ctags").unwrap();
        assert_eq!(
            compare_versions(spec, "2026.09.30+old", "2026.10.08+new").unwrap(),
            Ordering::Less
        );
        assert_eq!(
            compare_versions(spec, "2026.10.08+old", "2026.10.08+new").unwrap(),
            Ordering::Equal
        );
    }

    #[test]
    fn compare_rejects_malformed_versions() {
        for version in ["unsupported", "1.2.3.4", "1.0.01"] {
            assert!(compare_scc(version, "1.0.0").is_err());
        }
        let spec = super::super::catalog::lookup_tool("universal-ctags").unwrap();
        assert!(compare_versions(spec, "2026.02.30", "2026.03.01").is_err());
    }

    #[test]
    fn compare_identical_opaque_tags_as_unchanged() {
        assert_eq!(compare_scc("test", "test").unwrap(), Ordering::Equal);
    }
}
