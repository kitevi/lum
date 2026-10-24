use anyhow::{Context, Result, bail};
use std::path::Path;
use ureq::unversioned::multipart::{Form, Part};

// Public file host used by `lum backup`. This must work with the current
// upload/restore flow:
//  1. upload via multipart form field: file=@archive.tar.gz
//  2. return a direct URL to the uploaded file
//  3. allow restore by reconstructing: <download-base>/<code>.tar.gz
//
// Current default (verified 2026-03-29):
//
//     https://x0.at
//       - closest match to 0x0-style hosts
//       - worked
//       - 512 MiB max
//       - retention: 3–100 days
//
// Other candidates to keep in mind:
//
//     https://4d2.sh
//       - worked
//       - 1024 MiB max
//       - retention: 31–730 days
//       - NOTE: upload is https://4d2.sh but returned download URLs are on
//         https://dl.4d2.sh, so switching requires separate upload/download bases
//
//     https://curl-t.com
//       - worked
//       - curl-first UX
//       - site says files are stored forever
//       - NOTE: returned URLs include /<token>/<filename>, so switching requires
//         different code extraction + restore URL construction
//
// Works, but needs code changes:
//
//     https://catbox.moe/user/api.php
//       curl -F 'reqtype=fileupload' -F 'fileToUpload=@file.tar.gz' https://catbox.moe/user/api.php
//
//     https://litterbox.catbox.moe/resources/internals/api.php
//       - temporary only: 1h / 12h / 24h / 72h
//       curl -F 'reqtype=fileupload' -F 'time=24h' -F 'fileToUpload=@file.tar.gz' https://litterbox.catbox.moe/resources/internals/api.php
pub(crate) const BACKUP_SERVICE_URL: &str = "https://x0.at";

pub(crate) fn upload_archive(archive: &Path) -> Result<String> {
    let file_name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("archive.tar.gz")
        .to_owned();
    let part = Part::file(archive)
        .with_context(|| format!("failed to open archive {}", archive.display()))?
        .file_name(&file_name);
    let response = crate::github::agent()
        .post(BACKUP_SERVICE_URL)
        .send(Form::new().part("file", part))
        .context("failed to upload")?
        .body_mut()
        .read_to_string()
        .context("failed to read upload response")?;

    let url = clean_url(&response);
    if url.is_empty() {
        bail!("upload failed: empty response");
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        bail!("upload failed: invalid URL response: {url}");
    }
    Ok(url)
}

pub(crate) fn restore_url(code: &str) -> String {
    format!("{BACKUP_SERVICE_URL}/{code}")
}

pub(crate) fn code_from_url(url: &str) -> Result<String> {
    let path = url::Url::parse(url)
        .with_context(|| format!("failed to parse upload URL {url}"))?
        .path()
        .to_owned();
    // The code is whatever file name the host returned. Hosts may rename the
    // upload (x0.at truncates "archive.tar.gz" to "<id>.gz"), so restore must
    // reuse it verbatim instead of assuming a ".tar.gz" suffix.
    Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("upload failed: URL has no file name: {url}"))
}

fn clean_url(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{code_from_url, restore_url};

    // The code is the exact file name the host returned, so restore must
    // reconstruct the byte-identical URL regardless of how the host mangled
    // the uploaded file name (x0.at truncates ".tar.gz" to ".gz").
    #[test]
    fn url_round_trips_through_code() {
        for url in [
            "https://x0.at/jtPb.gz",     // x0.at: drops the ".tar" part
            "https://x0.at/Ab12.tar.gz", // host preserving the full suffix
            "https://x0.at/Q9zc",        // host stripping the suffix entirely
        ] {
            let code = code_from_url(url).expect(url);
            assert_eq!(restore_url(&code), url);
        }
    }

    #[test]
    fn code_from_url_rejects_url_without_file_name() {
        assert!(code_from_url("https://x0.at/").is_err());
    }
}
