//! Names complete from lum's catalogs and from the user's config files.

use assert_cmd::{Command, assert::Assert};
use predicates::str::contains;
use tempfile::TempDir;

/// What fish gets back for `line`, with lum's config files in `config`.
fn complete(config: &TempDir, line: &str) -> Assert {
    Command::cargo_bin("lum")
        .unwrap()
        .env("XDG_CONFIG_HOME", config.path())
        .args(["__complete_word__", "--shell", "fish", "--line", line])
        .assert()
        .success()
}

fn write_config(config: &TempDir, name: &str, content: &serde_json::Value) {
    let dir = config.path().join("lum");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), content.to_string()).unwrap();
}

#[test]
fn catalog_names_complete_with_their_descriptions() {
    let config = TempDir::new().unwrap();
    for (line, candidate) in [
        ("lum radio play ", "dron\tSomaFM Drone Zone"),
        ("lum apps install ", "mgba\tGame Boy Advance emulator"),
        ("lum font uninstall ", "dmca-sans-serif\t"),
        ("lum env unset ", "baro\tBARO_DB_URL"),
    ] {
        complete(&config, line).stdout(contains(candidate));
    }
}

#[test]
fn identities_complete_from_the_git_id_config() {
    let config = TempDir::new().unwrap();
    let folder = config.path().join("work");
    write_config(
        &config,
        "git-identities.json",
        &serde_json::json!({ "identities": [{
            "name": "work",
            "author_name": "Jane Doe",
            "email": "jane@company.com",
            "domain": "gitlab.company.com",
            "folders": [folder]
        }]}),
    );
    complete(&config, "lum git-id pubkey ").stdout(contains("work\tjane@company.com"));
}

#[test]
fn mirror_tags_complete_once_each() {
    let config = TempDir::new().unwrap();
    write_config(
        &config,
        "repos.json",
        &serde_json::json!({ "repos": [
            { "url": "https://example.com/a.git", "tags": ["client", "v2"] },
            { "url": "https://example.com/b.git", "tags": ["client"] }
        ]}),
    );
    let output = complete(&config, "lum repos mirror watch ")
        .get_output()
        .stdout
        .clone();
    let tags: Vec<String> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(tags, ["client", "v2"]);
}
