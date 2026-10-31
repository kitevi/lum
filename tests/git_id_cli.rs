use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use tempfile::TempDir;

fn lum_with_home(home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("XDG_DATA_HOME", home.path().join(".local/share"))
        .env("XDG_STATE_HOME", home.path().join(".local/state"))
        .env("GIT_CONFIG_NOSYSTEM", "1");
    cmd
}

fn write_git_id_config(home: &TempDir, content: &str) {
    let config_dir = home.path().join(".config/lum");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("git-identities.json"), content).unwrap();
}

fn circle_touch_config(
    home: &TempDir,
    password: &str,
    allow_insecure_http: Option<bool>,
) -> String {
    circle_touch_config_in(
        &home.path().join("Work/CircleTouch"),
        password,
        allow_insecure_http,
    )
}

fn circle_touch_config_in(
    folder: &std::path::Path,
    password: &str,
    allow_insecure_http: Option<bool>,
) -> String {
    let mut authentication = serde_json::json!({
        "type": "http-basic",
        "scheme": "http",
        "username": "jane",
        "password": password
    });
    if let Some(allow) = allow_insecure_http {
        authentication["allow_insecure_http"] = allow.into();
    }
    serde_json::json!({
        "identities": [{
            "name": "circletouch",
            "author_name": "Jane Doe",
            "email": "jane@company.com",
            "domain": "gitlab.dev.circletouch.eu",
            "folders": [folder],
            "authentication": authentication
        }]
    })
    .to_string()
}

fn ssh_identity(name: &str, domain: &str, folder: &std::path::Path) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "author_name": name,
        "email": format!("{name}@example.com"),
        "domain": domain,
        "folders": [folder]
    })
}

fn http_identity(name: &str, domain: &str, folder: &std::path::Path) -> serde_json::Value {
    let mut identity = ssh_identity(name, domain, folder);
    identity["authentication"] = serde_json::json!({
        "type": "http-basic",
        "scheme": "https",
        "username": format!("{name}-user"),
        "password": format!("{name}-secret")
    });
    identity
}

fn write_identities(home: &TempDir, identities: &[serde_json::Value]) {
    write_git_id_config(
        home,
        &serde_json::json!({ "identities": identities }).to_string(),
    );
}

fn sync_git_ids(home: &TempDir) {
    sync_command(home)
        .args(["git-id", "sync"])
        .assert()
        .success();
}

#[cfg(unix)]
fn install_fake_bin(home: &TempDir, name: &str, script: &str) {
    use std::os::unix::fs::PermissionsExt;

    let bin = home.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let path = bin.join(name);
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
fn path_with_fake_bins(home: &TempDir) -> std::ffi::OsString {
    let mut paths = vec![home.path().join("bin")];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    std::env::join_paths(paths).unwrap()
}

fn sync_command(home: &TempDir) -> Command {
    let mut command = lum_with_home(home);
    #[cfg(unix)]
    {
        install_fake_bin(
            home,
            "ssh-keygen",
            "#!/bin/sh\nif [ \"$1\" = \"-h\" ]; then exit 0; fi\nkey=\"\"\ncomment=\"\"\nwhile [ $# -gt 0 ]; do\n  case \"$1\" in\n    -f) shift; key=\"$1\" ;;\n    -C) shift; comment=\"$1\" ;;\n  esac\n  shift\ndone\nprintf 'PRIVATE %s\n' \"$comment\" > \"$key\"\nprintf 'ssh-ed25519 TESTKEY %s\n' \"$comment\" > \"$key.pub\"\n",
        );
        command.env("PATH", path_with_fake_bins(home));
    }
    command
}

fn credential_route(home: &TempDir, identity: &str) -> String {
    let config = std::fs::read_to_string(
        home.path()
            .join(format!(".gitconfig-lum-git-id-{identity}")),
    )
    .unwrap();
    config
        .lines()
        .find(|line| line.contains("__git_credential"))
        .and_then(|line| line.split_ascii_whitespace().last())
        .map(|route| route.trim_end_matches('"').to_owned())
        .expect("generated credential helper route")
}

fn git_with_home(home: &TempDir) -> Command {
    let mut command = Command::new("git");
    command
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("XDG_STATE_HOME", home.path().join(".local/state"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    command
}

#[test]
fn init_creates_sample_git_identity_config() {
    let home = TempDir::new().unwrap();

    lum_with_home(&home)
        .args(["git-id", "init"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Created"));

    let config_path = home.path().join(".config/lum/git-identities.json");
    let config = std::fs::read_to_string(&config_path).unwrap();

    assert!(config.contains("\"identities\""));
    assert!(config.contains("\"name\""));
    assert!(config.contains("\"author_name\""));
    assert!(config.contains("\"folders\""));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(config_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn config_path_prints_git_identity_config_location() {
    let home = TempDir::new().unwrap();

    lum_with_home(&home)
        .args(["git-id", "config-path"])
        .assert()
        .success()
        .stdout(predicates::str::contains("git-identities.json"));
}

#[test]
fn where_uses_most_specific_managed_folder() {
    let home = TempDir::new().unwrap();
    let config_dir = home.path().join(".config/lum");
    std::fs::create_dir_all(&config_dir).unwrap();
    let work = home.path().join("Work");
    let github = work.join("Github");
    let repo = github.join("project");
    std::fs::create_dir_all(&repo).unwrap();

    let config = format!(
        r#"{{
  "identities": [
    {{"name":"work","author_name":"Work User","email":"work@example.com","domain":"github.com","folders":["{}"]}},
    {{"name":"github-work","author_name":"Github User","email":"github@example.com","domain":"github.com","folders":["{}"]}}
  ]
}}"#,
        work.display(),
        github.display()
    );
    std::fs::write(config_dir.join("git-identities.json"), config).unwrap();

    let mut cmd = lum_with_home(&home);
    cmd.args(["git-id", "where"])
        .current_dir(&repo)
        .assert()
        .success()
        .stdout(predicates::str::contains("github-work"))
        .stdout(predicates::str::contains("Work User").not());
}

#[test]
fn sync_creates_managed_artifacts_from_config() {
    let home = TempDir::new().unwrap();
    let managed = home.path().join("Work/Github");
    let config = format!(
        r#"{{"identities":[{{"name":"github-work","author_name":"Jane Doe","email":"jane@company.com","domain":"github.com","folders":["{}"]}}]}}"#,
        managed.display()
    );
    write_git_id_config(&home, &config);
    std::fs::create_dir_all(home.path().join(".ssh")).unwrap();
    std::fs::write(
        home.path().join(".ssh/config"),
        "Host existing\n  User git\n",
    )
    .unwrap();

    sync_command(&home)
        .args(["git-id", "sync"])
        .assert()
        .success()
        .stdout(predicates::str::contains("github-work"));

    assert!(managed.exists());
    let public_key =
        std::fs::read_to_string(home.path().join(".ssh/lum-git-id-github-work.pub")).unwrap();
    assert!(public_key.contains("[lum:git-id identity=github-work]"));
    let git_config =
        std::fs::read_to_string(home.path().join(".gitconfig-lum-git-id-github-work")).unwrap();
    assert!(git_config.contains("# lum:git-id:managed identity=github-work"));
    assert!(git_config.contains("signingkey"));
    assert!(git_config.contains("insteadOf = https://github.com/"));
    let global_git_config = std::fs::read_to_string(home.path().join(".gitconfig")).unwrap();
    assert!(global_git_config.contains("# lum:git-id:begin"));
    assert!(global_git_config.contains("includeIf"));
    assert!(!global_git_config.starts_with('\n'));
    assert_eq!(
        std::fs::read_to_string(home.path().join(".ssh/config")).unwrap(),
        "Host existing\n  User git\n"
    );
    let allowed_signers =
        std::fs::read_to_string(home.path().join(".ssh/allowed_signers")).unwrap();
    assert!(!allowed_signers.starts_with('\n'));
}

#[test]
fn sync_configures_http_basic_credentials_without_copying_the_password() {
    let home = TempDir::new().unwrap();
    write_git_id_config(&home, &circle_touch_config(&home, "TOP-SECRET", Some(true)));
    sync_git_ids(&home);

    let git_config =
        std::fs::read_to_string(home.path().join(".gitconfig-lum-git-id-circletouch")).unwrap();
    assert!(git_config.contains("[credential \"http://gitlab.dev.circletouch.eu\"]"));
    assert!(git_config.contains("username = \"jane\""));
    assert!(git_config.contains("__git_credential"));
    assert!(!git_config.contains("TOP-SECRET"));
    assert!(!git_config.contains("sshCommand"));
    assert!(!git_config.contains("insteadOf"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let config_path = home.path().join(".config/lum/git-identities.json");
        assert_eq!(
            std::fs::metadata(config_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    assert!(!home.path().join(".ssh/config").exists());
}

#[cfg(unix)]
#[test]
fn git_identity_routing_survives_a_symlinked_folder_path() {
    let home = TempDir::new().unwrap();
    let managed = home.path().join("Work/CircleTouch/project");
    std::fs::create_dir_all(home.path().join("Work/CircleTouch")).unwrap();
    let symlink = home.path().join("linked");
    std::os::unix::fs::symlink(home.path().join("Work"), &symlink).unwrap();

    write_git_id_config(
        &home,
        &circle_touch_config_in(&symlink.join("CircleTouch"), "TOP-SECRET", Some(true)),
    );
    sync_git_ids(&home);
    git_with_home(&home)
        .args(["init", managed.to_str().unwrap()])
        .assert()
        .success();

    git_with_home(&home)
        .args(["credential", "fill"])
        .current_dir(&managed)
        .write_stdin("protocol=http\nhost=gitlab.dev.circletouch.eu\n\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("username=jane\n"))
        .stdout(predicates::str::contains("password=TOP-SECRET\n"));
}

#[cfg(unix)]
#[test]
fn symlinked_git_identity_routes_use_literal_folder_paths() {
    for (folder, sibling) in [
        ("Work[1]", "Work1"),
        ("Work*", "Work-other"),
        ("Work?", "Work1"),
        ("Work\"quoted", "Workquoted"),
        (r"Work\1", "Work/1"),
    ] {
        let home = TempDir::new().unwrap();
        let real = home.path().join(folder);
        std::fs::create_dir_all(&real).unwrap();
        let linked = home.path().join("linked");
        std::os::unix::fs::symlink(&real, &linked).unwrap();
        write_git_id_config(
            &home,
            &circle_touch_config_in(&linked, "TOP-SECRET", Some(true)),
        );
        sync_git_ids(&home);

        let managed = real.join("project");
        let unmanaged = home.path().join(sibling).join("project");
        for repo in [&managed, &unmanaged] {
            git_with_home(&home)
                .arg("init")
                .arg(repo)
                .assert()
                .success();
        }
        git_with_home(&home)
            .args(["config", "--get", "user.name"])
            .current_dir(&unmanaged)
            .assert()
            .code(1)
            .stdout("");
        git_with_home(&home)
            .args(["config", "--get", "user.name"])
            .current_dir(&managed)
            .assert()
            .success()
            .stdout("Jane Doe\n");
    }
}

#[cfg(unix)]
#[test]
fn sync_rejects_newlines_in_symlink_targets_without_damaging_git_config() {
    let home = TempDir::new().unwrap();
    let real = home.path().join("Work\n1");
    std::fs::create_dir_all(&real).unwrap();
    let linked = home.path().join("linked");
    std::os::unix::fs::symlink(&real, &linked).unwrap();
    write_git_id_config(
        &home,
        &circle_touch_config_in(&linked, "TOP-SECRET", Some(true)),
    );
    git_with_home(&home)
        .args(["config", "--global", "user.name", "Original Identity"])
        .assert()
        .success();

    sync_command(&home)
        .args(["git-id", "sync"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("newline"));

    git_with_home(&home)
        .args(["config", "--global", "--get", "user.name"])
        .assert()
        .success()
        .stdout("Original Identity\n");
}

#[test]
fn git_credential_fill_uses_the_folder_scoped_http_credential() {
    let home = TempDir::new().unwrap();
    let managed = home.path().join("Work/CircleTouch/project");
    write_git_id_config(&home, &circle_touch_config(&home, "TOP-SECRET", Some(true)));
    sync_git_ids(&home);

    git_with_home(&home)
        .args(["init", managed.to_str().unwrap()])
        .assert()
        .success();

    git_with_home(&home)
        .args(["credential", "fill"])
        .current_dir(&managed)
        .write_stdin("protocol=http\nhost=gitlab.dev.circletouch.eu\n\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("username=jane\n"))
        .stdout(predicates::str::contains("password=TOP-SECRET\n"));
}

#[test]
fn sync_refuses_plain_http_credentials_without_explicit_acknowledgement() {
    let home = TempDir::new().unwrap();
    write_git_id_config(&home, &circle_touch_config(&home, "TOP-SECRET", None));

    sync_command(&home)
        .args(["git-id", "sync"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("allow_insecure_http"))
        .stderr(predicates::str::contains("TOP-SECRET").not());

    assert!(
        !home
            .path()
            .join(".gitconfig-lum-git-id-circletouch")
            .exists()
    );
}

#[cfg(unix)]
#[test]
fn credential_helper_refuses_to_expose_a_world_readable_secret() {
    use std::os::unix::fs::PermissionsExt;

    let home = TempDir::new().unwrap();
    let managed = home.path().join("Work/CircleTouch/project");
    write_git_id_config(&home, &circle_touch_config(&home, "TOP-SECRET", Some(true)));
    sync_git_ids(&home);
    git_with_home(&home)
        .args(["init", managed.to_str().unwrap()])
        .assert()
        .success();

    let config_path = home.path().join(".config/lum/git-identities.json");
    let mut permissions = std::fs::metadata(&config_path).unwrap().permissions();
    permissions.set_mode(0o644);
    std::fs::set_permissions(config_path, permissions).unwrap();

    git_with_home(&home)
        .args(["credential", "fill"])
        .current_dir(&managed)
        .write_stdin("protocol=http\nhost=gitlab.dev.circletouch.eu\n\n")
        .assert()
        .failure()
        .stdout(predicates::str::contains("TOP-SECRET").not())
        .stderr(predicates::str::contains("permissions are not private"))
        .stderr(predicates::str::contains("TOP-SECRET").not());
}

#[test]
fn info_describes_http_authentication_without_printing_the_password() {
    let home = TempDir::new().unwrap();
    write_git_id_config(&home, &circle_touch_config(&home, "TOP-SECRET", Some(true)));

    lum_with_home(&home)
        .args(["git-id", "info", "circletouch"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Authentication: HTTP Basic (http://gitlab.dev.circletouch.eu)",
        ))
        .stdout(predicates::str::contains("Username: jane"))
        .stdout(predicates::str::contains("Password: configured"))
        .stdout(predicates::str::contains("TOP-SECRET").not());
}

#[test]
fn credential_helper_does_not_expose_a_secret_to_the_wrong_context() {
    let home = TempDir::new().unwrap();
    write_git_id_config(&home, &circle_touch_config(&home, "TOP-SECRET", Some(true)));
    sync_git_ids(&home);
    let route = credential_route(&home, "circletouch");

    for request in [
        "protocol=https\nhost=gitlab.dev.circletouch.eu\nusername=jane\n\n",
        "protocol=http\nhost=example.com\nusername=jane\n\n",
        "protocol=http\nhost=gitlab.dev.circletouch.eu\nusername=someone-else\n\n",
    ] {
        lum_with_home(&home)
            .args(["__git_credential", &route, "get"])
            .write_stdin(request)
            .assert()
            .failure()
            .stdout(predicates::str::contains("TOP-SECRET").not())
            .stderr(predicates::str::contains("TOP-SECRET").not());
    }
}

#[test]
fn credential_store_and_erase_leave_the_declarative_config_unchanged() {
    let home = TempDir::new().unwrap();
    write_git_id_config(&home, &circle_touch_config(&home, "TOP-SECRET", Some(true)));
    sync_git_ids(&home);
    let route = credential_route(&home, "circletouch");
    let config_path = home.path().join(".config/lum/git-identities.json");
    let before = std::fs::read_to_string(&config_path).unwrap();
    let request =
        "protocol=http\nhost=gitlab.dev.circletouch.eu\nusername=jane\npassword=TOP-SECRET\n\n";

    lum_with_home(&home)
        .args(["__git_credential", &route, "store"])
        .write_stdin(request)
        .assert()
        .success()
        .stdout(predicates::str::is_empty());
    lum_with_home(&home)
        .args(["__git_credential", &route, "erase"])
        .write_stdin(request)
        .assert()
        .success()
        .stdout(predicates::str::is_empty())
        .stderr(predicates::str::contains(
            "credentials for identity circletouch were rejected",
        ))
        .stderr(predicates::str::contains("TOP-SECRET").not());

    assert_eq!(std::fs::read_to_string(config_path).unwrap(), before);
}

#[test]
fn credential_helper_does_not_initialize_application_logging() {
    let home = TempDir::new().unwrap();
    write_git_id_config(&home, &circle_touch_config(&home, "TOP-SECRET", Some(true)));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = home.path().join(".config/lum/git-identities.json");
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(path, permissions).unwrap();
    }

    lum_with_home(&home)
        .args([
            "__git_credential",
            "db7aca6b4d6fbb3e63219d8119ff67a5c79f883a96c8e3eeaf5db3755f761add",
            "get",
        ])
        .write_stdin("protocol=http\nhost=gitlab.dev.circletouch.eu\nusername=jane\n\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("password=TOP-SECRET\n"));

    assert!(!home.path().join(".local/state/lum/logs").exists());
}

#[test]
fn changing_only_the_password_takes_effect_without_another_sync() {
    let home = TempDir::new().unwrap();
    let managed = home.path().join("Work/CircleTouch/project");
    write_git_id_config(&home, &circle_touch_config(&home, "OLD-SECRET", Some(true)));
    sync_git_ids(&home);
    git_with_home(&home)
        .args(["init", managed.to_str().unwrap()])
        .assert()
        .success();

    write_git_id_config(&home, &circle_touch_config(&home, "NEW-SECRET", Some(true)));

    git_with_home(&home)
        .args(["credential", "fill"])
        .current_dir(&managed)
        .write_stdin("protocol=http\nhost=gitlab.dev.circletouch.eu\n\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("password=NEW-SECRET\n"))
        .stdout(predicates::str::contains("OLD-SECRET").not());
}

#[test]
fn sync_removes_http_routing_when_an_identity_returns_to_ssh() {
    let home = TempDir::new().unwrap();
    write_git_id_config(&home, &circle_touch_config(&home, "TOP-SECRET", Some(true)));
    sync_git_ids(&home);

    let identity_config_path = home.path().join(".gitconfig-lum-git-id-circletouch");
    let first = std::fs::read_to_string(&identity_config_path).unwrap();
    sync_git_ids(&home);
    assert_eq!(
        std::fs::read_to_string(&identity_config_path).unwrap(),
        first
    );

    let ssh_config = serde_json::json!({
        "identities": [{
            "name": "circletouch",
            "author_name": "Jane Doe",
            "email": "jane@company.com",
            "domain": "gitlab.dev.circletouch.eu",
            "folders": [home.path().join("Work/CircleTouch")]
        }]
    })
    .to_string();
    write_git_id_config(&home, &ssh_config);
    sync_git_ids(&home);

    let identity_config = std::fs::read_to_string(identity_config_path).unwrap();
    assert!(identity_config.contains("sshCommand"));
    assert!(identity_config.contains("insteadOf"));
    assert!(!identity_config.contains("__git_credential"));
    assert!(!identity_config.contains("TOP-SECRET"));
    assert!(!home.path().join(".ssh/config").exists());
}

#[test]
fn sync_removes_the_legacy_ssh_section_and_keeps_user_entries() {
    let home = TempDir::new().unwrap();
    write_identities(
        &home,
        &[ssh_identity(
            "work",
            "git.invalid",
            &home.path().join("Work"),
        )],
    );
    let ssh_dir = home.path().join(".ssh");
    std::fs::create_dir_all(&ssh_dir).unwrap();
    std::fs::write(
        ssh_dir.join("config"),
        "Host existing\n  User git\n\n# lum:git-id:begin\nHost git.invalid\n  IdentityFile ~/.ssh/lum-git-id-work\n\n# lum:git-id:end\n\nHost later\n  User me\n",
    )
    .unwrap();

    sync_git_ids(&home);

    let ssh_config = std::fs::read_to_string(ssh_dir.join("config")).unwrap();
    assert!(ssh_config.contains("Host existing\n  User git\n"));
    assert!(ssh_config.contains("Host later\n  User me\n"));
    assert!(!ssh_config.contains("lum:git-id"));
    assert!(!ssh_config.contains("git.invalid"));

    std::fs::write(
        ssh_dir.join("config"),
        "# lum:git-id:begin\nHost git.invalid\n\n# lum:git-id:end\n",
    )
    .unwrap();
    sync_git_ids(&home);
    assert_eq!(std::fs::read_to_string(ssh_dir.join("config")).unwrap(), "");
}

#[test]
fn sync_leaves_an_ssh_config_without_a_lum_section_byte_identical() {
    let home = TempDir::new().unwrap();
    write_identities(
        &home,
        &[ssh_identity(
            "work",
            "git.invalid",
            &home.path().join("Work"),
        )],
    );
    let ssh_dir = home.path().join(".ssh");
    std::fs::create_dir_all(&ssh_dir).unwrap();
    let original = "Host existing\r\n  User git";
    std::fs::write(ssh_dir.join("config"), original).unwrap();

    sync_git_ids(&home);

    assert_eq!(
        std::fs::read_to_string(ssh_dir.join("config")).unwrap(),
        original
    );
}

#[test]
fn sync_is_idempotent_and_keeps_user_settings_ahead_of_the_managed_section() {
    let home = TempDir::new().unwrap();
    write_identities(
        &home,
        &[ssh_identity(
            "work",
            "git.invalid",
            &home.path().join("Work"),
        )],
    );
    let gitconfig = home.path().join(".gitconfig");
    std::fs::write(&gitconfig, "[alias]\r\n  st = status\r\n").unwrap();

    sync_git_ids(&home);
    let read_generated = || {
        [
            ".gitconfig",
            ".gitconfig-lum-git-id-work",
            ".ssh/allowed_signers",
        ]
        .map(|name| std::fs::read_to_string(home.path().join(name)).unwrap())
    };
    let first = read_generated();
    assert!(first[0].starts_with("[alias]\r\n  st = status"));
    assert!(first[0].ends_with("# lum:git-id:end\n"));

    sync_git_ids(&home);
    assert_eq!(read_generated(), first);
}

#[test]
fn unbalanced_markers_stop_sync_without_touching_the_file() {
    for (file, content) in [
        (
            ".gitconfig",
            "[alias]\n  st = status\n# lum:git-id:begin\n[alias]\n  co = checkout\n",
        ),
        (".gitconfig", "[alias]\n  st = status\n# lum:git-id:end\n"),
        (
            ".gitconfig",
            "# lum:git-id:begin\n# lum:git-id:begin\n# lum:git-id:end\n",
        ),
        (
            ".ssh/config",
            "Host existing\n# lum:git-id:begin\nHost after\n  User me\n",
        ),
    ] {
        let home = TempDir::new().unwrap();
        write_identities(
            &home,
            &[ssh_identity(
                "work",
                "git.invalid",
                &home.path().join("Work"),
            )],
        );
        let path = home.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();

        sync_command(&home)
            .args(["git-id", "sync"])
            .assert()
            .failure()
            .stderr(predicates::str::contains(file))
            .stderr(predicates::str::contains("fix the file by hand"));

        assert_eq!(std::fs::read_to_string(&path).unwrap(), content, "{file}");
    }
}

#[test]
fn every_orphaned_identity_gets_its_own_private_backup() {
    let home = TempDir::new().unwrap();
    let keep = ssh_identity("keep", "git.invalid", &home.path().join("Keep"));
    write_identities(
        &home,
        &[
            keep.clone(),
            ssh_identity("gone1", "git.invalid", &home.path().join("Gone1")),
            ssh_identity("gone2", "git.invalid", &home.path().join("Gone2")),
        ],
    );
    sync_git_ids(&home);
    let private_key = std::fs::read_to_string(home.path().join(".ssh/lum-git-id-gone2")).unwrap();

    write_identities(&home, &[keep]);
    sync_git_ids(&home);

    let mut recovered = std::collections::BTreeMap::new();
    let backups = home.path().join(".local/share/lum/git-id/backups");
    for entry in std::fs::read_dir(&backups).unwrap() {
        let path = entry.unwrap().path();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600,
                "{}",
                path.display()
            );
        }
        let archive = flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap());
        for entry in tar::Archive::new(archive).entries().unwrap() {
            let mut entry = entry.unwrap();
            let name = entry.path().unwrap().display().to_string();
            let mut content = String::new();
            std::io::Read::read_to_string(&mut entry, &mut content).unwrap();
            recovered.insert(name, content);
        }
    }

    assert_eq!(
        recovered.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            ".gitconfig-lum-git-id-gone1",
            ".gitconfig-lum-git-id-gone2",
            "lum-git-id-gone1",
            "lum-git-id-gone1.pub",
            "lum-git-id-gone2",
            "lum-git-id-gone2.pub",
        ]
    );
    assert_eq!(recovered["lum-git-id-gone2"], private_key);
    assert!(!home.path().join(".ssh/lum-git-id-gone1").exists());
    assert!(!home.path().join(".gitconfig-lum-git-id-gone2").exists());
    assert!(home.path().join(".ssh/lum-git-id-keep").exists());
}

#[cfg(unix)]
#[test]
fn sync_tightens_backups_written_by_older_versions() {
    use std::os::unix::fs::PermissionsExt;

    let home = TempDir::new().unwrap();
    write_identities(
        &home,
        &[ssh_identity(
            "work",
            "git.invalid",
            &home.path().join("Work"),
        )],
    );
    let backups = home.path().join(".local/share/lum/git-id/backups");
    std::fs::create_dir_all(&backups).unwrap();
    let old_archive = backups.join("git-id-orphans-1.tar.gz");
    std::fs::write(&old_archive, "archive").unwrap();
    std::fs::set_permissions(&old_archive, std::fs::Permissions::from_mode(0o644)).unwrap();

    sync_git_ids(&home);

    assert_eq!(
        std::fs::metadata(old_archive).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn dotted_identity_names_sync_and_find_their_keys() {
    let home = TempDir::new().unwrap();
    write_identities(
        &home,
        &[
            ssh_identity("acme.corp", "git.invalid", &home.path().join("Corp")),
            ssh_identity("acme.other", "git.invalid", &home.path().join("Other")),
        ],
    );

    sync_git_ids(&home);
    sync_git_ids(&home);

    lum_with_home(&home)
        .args(["git-id", "status"])
        .assert()
        .success()
        .stdout(predicates::str::contains("acme.corp  active"))
        .stdout(predicates::str::contains("acme.other  active"));
    for name in ["acme.corp", "acme.other"] {
        lum_with_home(&home)
            .args(["git-id", "pubkey", name])
            .assert()
            .success()
            .stdout(predicates::str::contains(format!(
                "[lum:git-id identity={name}]"
            )));
    }
}

#[test]
fn identity_names_that_cannot_be_file_names_are_rejected_before_anything_is_written() {
    for name in ["a/b", r"a\b", "line\nbreak", "tab\tbed", "key.pub"] {
        let home = TempDir::new().unwrap();
        write_identities(
            &home,
            &[ssh_identity(name, "git.invalid", &home.path().join("Work"))],
        );
        let gitconfig = home.path().join(".gitconfig");
        std::fs::write(&gitconfig, "[user]\n  name = Original\n").unwrap();

        sync_command(&home)
            .args(["git-id", "sync"])
            .assert()
            .failure()
            .stderr(predicates::str::contains("identity name"));

        assert_eq!(
            std::fs::read_to_string(&gitconfig).unwrap(),
            "[user]\n  name = Original\n"
        );
        assert!(!home.path().join(".ssh").exists(), "{name:?}");
    }
}

#[test]
fn author_names_reach_git_unchanged() {
    for author in [
        r#"Jane "JD" Doe"#,
        "Jane Doe #1",
        "Jane Doe; CTO",
        r"Jane\Doe",
        "Zoë O'Brien",
    ] {
        let home = TempDir::new().unwrap();
        let folder = home.path().join("Work");
        let mut identity = ssh_identity("work", "git.invalid", &folder);
        identity["author_name"] = author.into();
        write_identities(&home, &[identity]);
        sync_git_ids(&home);

        let repo = folder.join("repo");
        git_with_home(&home)
            .arg("init")
            .arg(&repo)
            .assert()
            .success();
        git_with_home(&home)
            .args(["config", "--get", "user.name"])
            .current_dir(&repo)
            .assert()
            .success()
            .stdout(format!("{author}\n"));
    }
}

#[cfg(unix)]
fn assert_git_runs_ssh_with_key(
    home: &TempDir,
    repo: &std::path::Path,
    private_key: &std::path::Path,
) {
    install_fake_bin(
        home,
        "ssh",
        "#!/bin/sh\nfor arg in \"$@\"; do printf '%s\\n' \"$arg\" >> \"$HOME/ssh.log\"; done\nexit 1\n",
    );
    git_with_home(home)
        .env("PATH", path_with_fake_bins(home))
        .args(["ls-remote", "ssh://git@git.invalid/o/r"])
        .current_dir(repo)
        .assert()
        .failure();

    let ssh_log = std::fs::read_to_string(home.path().join("ssh.log")).unwrap();
    let arguments: Vec<_> = ssh_log.lines().take(4).collect();
    assert_eq!(
        arguments,
        [
            "-i",
            private_key.to_str().unwrap(),
            "-o",
            "IdentitiesOnly=yes"
        ]
    );
}

#[cfg(unix)]
#[test]
fn unusual_identity_names_keep_working() {
    for name in [
        "my work",
        "jane@work",
        "team;a",
        "team#a",
        "foo\"bar",
        "x$(touch PWNED)",
        "työ",
    ] {
        let home = TempDir::new().unwrap();
        let folder = home.path().join("Work");
        let mut identity = ssh_identity(name, "git.invalid", &folder);
        identity["email"] = "work@example.com".into();
        write_identities(&home, &[identity]);
        sync_git_ids(&home);
        sync_git_ids(&home);

        lum_with_home(&home)
            .args(["git-id", "status"])
            .assert()
            .success()
            .stdout(format!("{name}  active\n"));
        let repo = folder.join("repo");
        git_with_home(&home)
            .arg("init")
            .arg(&repo)
            .assert()
            .success();
        git_with_home(&home)
            .args(["config", "--get", "user.email"])
            .current_dir(&repo)
            .assert()
            .success()
            .stdout("work@example.com\n");
        lum_with_home(&home)
            .args(["git-id", "where"])
            .current_dir(&repo)
            .assert()
            .success()
            .stdout(predicates::str::contains(format!("Identity: {name}\n")));

        let private_key = home.path().join(".ssh").join(format!("lum-git-id-{name}"));
        assert_git_runs_ssh_with_key(&home, &repo, &private_key);
        assert!(!repo.join("PWNED").exists());
        assert!(!home.path().join(".ssh/config").exists());
    }
}

#[cfg(unix)]
#[test]
fn generated_config_survives_unusual_home_paths() {
    for prefix in [
        "First Last ",
        "a#b;c ",
        "it's ",
        "say \"hi\" ",
        r"back\slash ",
    ] {
        let home = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
        write_identities(
            &home,
            &[ssh_identity(
                "work",
                "git.invalid",
                &home.path().join("Work"),
            )],
        );
        sync_git_ids(&home);

        let repo = home.path().join("Work/repo");
        git_with_home(&home)
            .arg("init")
            .arg(&repo)
            .assert()
            .success();
        git_with_home(&home)
            .args(["config", "--get", "user.email"])
            .current_dir(&repo)
            .assert()
            .success()
            .stdout("work@example.com\n");
        assert_git_runs_ssh_with_key(&home, &repo, &home.path().join(".ssh/lum-git-id-work"));
    }
}

#[test]
fn domains_must_be_plain_hosts() {
    let home = TempDir::new().unwrap();
    let folder = home.path().join("Work");
    for domain in [
        "git.invalid\nProxyCommand=true",
        "git\tinvalid",
        "@git.invalid",
        ":@git.invalid",
        "jane@git.invalid",
        "git.invalid:",
        "git\"x.invalid",
        "git invalid",
        "https://git.invalid",
        "git.invalid/path",
        "git.invalid?query",
        "git.invalid#fragment",
    ] {
        write_identities(&home, &[ssh_identity("work", domain, &folder)]);
        lum_with_home(&home)
            .args(["git-id", "info", "work"])
            .assert()
            .failure()
            .stderr(predicates::str::contains("domain must be"));
    }
    for domain in [
        "git.invalid",
        "git.invalid:2222",
        "git.invalid:443",
        "my_host.invalid",
        "127.0.0.1:8080",
        "[::1]:2222",
    ] {
        write_identities(&home, &[ssh_identity("work", domain, &folder)]);
        lum_with_home(&home)
            .args(["git-id", "info", "work"])
            .assert()
            .success()
            .stdout(predicates::str::contains(format!("Domain:   {domain}\n")));
    }
}

#[test]
fn uppercase_domains_keep_working_and_get_the_https_rewrite() {
    let home = TempDir::new().unwrap();
    let folder = home.path().join("Work");
    write_identities(&home, &[ssh_identity("work", "GIT.Invalid", &folder)]);

    lum_with_home(&home)
        .args(["git-id", "info", "work"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Domain:   git.invalid\n"));

    sync_git_ids(&home);
    let repo = folder.join("repo");
    git_with_home(&home)
        .arg("init")
        .arg(&repo)
        .assert()
        .success();
    git_with_home(&home)
        .args(["ls-remote", "--get-url", "https://git.invalid/o/r"])
        .current_dir(&repo)
        .assert()
        .success()
        .stdout("ssh://git@git.invalid/o/r\n");
}

#[test]
fn emails_must_be_safe_in_allowed_signers() {
    let home = TempDir::new().unwrap();
    for email in [
        "jane doe@example.com",
        "jane\"@example.com",
        "a,b@example.com",
    ] {
        let mut identity = ssh_identity("work", "git.invalid", &home.path().join("Work"));
        identity["email"] = email.into();
        write_identities(&home, &[identity]);
        lum_with_home(&home)
            .args(["git-id", "info", "work"])
            .assert()
            .failure()
            .stderr(predicates::str::contains("email must not contain"));
    }
}

#[test]
fn unknown_config_fields_are_rejected_without_echoing_values() {
    let home = TempDir::new().unwrap();
    let folder = home.path().join("Work");

    let mut identity = ssh_identity("work", "git.invalid", &folder);
    identity["authentification"] = serde_json::json!({
        "type": "http-basic", "scheme": "https", "username": "j", "password": "TOP-SECRET"
    });
    write_identities(&home, &[identity]);
    lum_with_home(&home)
        .args(["git-id", "info", "work"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("authentification"))
        .stderr(predicates::str::contains("TOP-SECRET").not());

    let mut identity = ssh_identity("work", "git.invalid", &folder);
    identity["authentication"] = serde_json::json!({
        "type": "http-basic", "scheme": "https", "username": "j", "pasword": "TOP-SECRET"
    });
    write_identities(&home, &[identity]);
    lum_with_home(&home)
        .args(["git-id", "info", "work"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("password"))
        .stderr(predicates::str::contains("TOP-SECRET").not());

    write_git_id_config(&home, r#"{"identities":[],"identitys":[]}"#);
    lum_with_home(&home)
        .args(["git-id", "status"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("identitys"));
}

#[test]
fn folders_must_be_absolute_or_start_with_a_tilde_slash() {
    for folder in ["Work", "./Work", "~", "~other/Work", "$HOME/Work"] {
        let home = TempDir::new().unwrap();
        let cwd = home.path().join("cwd");
        std::fs::create_dir_all(&cwd).unwrap();
        write_identities(
            &home,
            &[ssh_identity(
                "work",
                "git.invalid",
                std::path::Path::new(folder),
            )],
        );

        sync_command(&home)
            .args(["git-id", "sync"])
            .current_dir(&cwd)
            .assert()
            .failure()
            .stderr(predicates::str::contains("must be an absolute path"));

        assert_eq!(std::fs::read_dir(&cwd).unwrap().count(), 0, "{folder}");
    }

    let home = TempDir::new().unwrap();
    write_identities(
        &home,
        &[ssh_identity(
            "work",
            "git.invalid",
            std::path::Path::new("~/Work"),
        )],
    );
    sync_git_ids(&home);
    assert!(home.path().join("Work").is_dir());
}

#[test]
fn https_credentials_cannot_be_nested_under_ssh_on_the_same_domain() {
    let home = TempDir::new().unwrap();
    let outer_folder = home.path().join("Work");
    write_identities(
        &home,
        &[
            ssh_identity("outer", "a.invalid", &outer_folder),
            http_identity("inner", "a.invalid", &outer_folder.join("Inner")),
        ],
    );

    sync_command(&home)
        .args(["git-id", "sync"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("is inside folder"))
        .stderr(predicates::str::contains("would never be used"));

    assert!(!home.path().join(".gitconfig").exists());
}

#[test]
fn other_nestings_under_an_ssh_identity_sync_with_a_warning() {
    let home = TempDir::new().unwrap();
    let outer_folder = home.path().join("Work");
    let inner_folder = outer_folder.join("Inner");
    let repo = inner_folder.join("repo");
    let outer = ssh_identity("outer", "a.invalid", &outer_folder);
    let mut plain_http = http_identity("inner", "a.invalid", &inner_folder);
    plain_http["authentication"]["scheme"] = "http".into();
    plain_http["authentication"]["allow_insecure_http"] = true.into();

    for inner in [
        http_identity("inner", "b.invalid", &inner_folder),
        ssh_identity("inner", "b.invalid", &inner_folder),
        plain_http,
    ] {
        write_identities(&home, &[outer.clone(), inner]);
        sync_command(&home)
            .args(["git-id", "sync"])
            .assert()
            .success()
            .stderr(predicates::str::contains("warning: identity inner"))
            .stderr(predicates::str::contains("still apply there"));
        git_with_home(&home)
            .arg("init")
            .arg(&repo)
            .assert()
            .success();
        git_with_home(&home)
            .args(["config", "--get", "user.email"])
            .current_dir(&repo)
            .assert()
            .success()
            .stdout("inner@example.com\n");
    }

    // The rewrite only matches https://, so the plain HTTP identity keeps its credentials.
    git_with_home(&home)
        .args(["ls-remote", "--get-url", "http://a.invalid/o/r"])
        .current_dir(&repo)
        .assert()
        .success()
        .stdout("http://a.invalid/o/r\n");
    git_with_home(&home)
        .args(["credential", "fill"])
        .current_dir(&repo)
        .write_stdin("protocol=http\nhost=a.invalid\n\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("password=inner-secret\n"));
}

#[test]
fn credential_helper_keeps_working_when_other_config_rules_fail() {
    let home = TempDir::new().unwrap();
    let config = circle_touch_config(&home, "TOP-SECRET", Some(true));
    write_git_id_config(&home, &config);
    sync_git_ids(&home);
    let route = credential_route(&home, "circletouch");

    let mut config: serde_json::Value = serde_json::from_str(&config).unwrap();
    config["_comment"] = "kept by hand".into();
    config["identities"][0]["note"] = "kept by hand".into();
    config["identities"][0]["folders"] = serde_json::json!(["Work"]);
    write_git_id_config(&home, &config.to_string());

    lum_with_home(&home)
        .args(["git-id", "status"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("unknown field"));
    lum_with_home(&home)
        .args(["__git_credential", &route, "get"])
        .write_stdin("protocol=http\nhost=gitlab.dev.circletouch.eu\n\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("password=TOP-SECRET\n"));
}

#[test]
fn sync_migrates_what_older_versions_generated() {
    let home = TempDir::new().unwrap();
    let folder = home.path().join("Work");
    let root = home.path().display();
    write_identities(&home, &[ssh_identity("work", "git.invalid", &folder)]);

    let ssh_dir = home.path().join(".ssh");
    std::fs::create_dir_all(&ssh_dir).unwrap();
    std::fs::create_dir_all(&folder).unwrap();
    let private_key = "PRIVATE work@example.com [lum:git-id identity=work]\n";
    let public_key = "ssh-ed25519 OLDKEY work@example.com [lum:git-id identity=work]\n";
    std::fs::write(ssh_dir.join("lum-git-id-work"), private_key).unwrap();
    std::fs::write(ssh_dir.join("lum-git-id-work.pub"), public_key).unwrap();
    std::fs::write(
        ssh_dir.join("config"),
        format!(
            "Host mine\n  User me\n\n# lum:git-id:begin\nHost git.invalid\n  HostName git.invalid\n  User git\n  IdentityFile {root}/.ssh/lum-git-id-work\n  IdentitiesOnly yes\n\n# lum:git-id:end\n"
        ),
    )
    .unwrap();
    std::fs::write(
        ssh_dir.join("allowed_signers"),
        "# lum:git-id:begin\nwork@example.com ssh-ed25519 OLDKEY\n# lum:git-id:end\n",
    )
    .unwrap();
    std::fs::write(
        home.path().join(".gitconfig"),
        format!(
            "[alias]\n  st = status\n\n# lum:git-id:begin\n[includeIf \"gitdir:{root}/Work/\"]\n  path = {root}/.gitconfig-lum-git-id-work\n\n# lum:git-id:end\n"
        ),
    )
    .unwrap();
    std::fs::write(
        home.path().join(".gitconfig-lum-git-id-work"),
        format!(
            "# lum:git-id:managed identity=work\n\n[user]\n  name = work\n  email = work@example.com\n  signingkey = {root}/.ssh/lum-git-id-work.pub\n\n[commit]\n  gpgsign = true\n\n[gpg]\n  format = ssh\n\n[gpg \"ssh\"]\n  allowedSignersFile = {root}/.ssh/allowed_signers\n\n[core]\n  sshCommand = \"ssh -i {root}/.ssh/lum-git-id-work -o IdentitiesOnly=yes\"\n\n[url \"ssh://git@git.invalid/\"]\n  insteadOf = https://git.invalid/\n"
        ),
    )
    .unwrap();

    lum_with_home(&home)
        .args(["git-id", "status"])
        .assert()
        .success()
        .stdout("work  needs sync (git config out of date)\n");

    sync_command(&home)
        .args(["git-id", "sync"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Removed lum's Host section"))
        .stdout(predicates::str::contains("Host git.invalid\n"))
        .stdout(predicates::str::contains(format!(
            "IdentityFile {root}/.ssh/lum-git-id-work\n"
        )));

    // Keys survive, so nothing needs uploading again and old signatures still verify.
    assert_eq!(
        std::fs::read_to_string(ssh_dir.join("lum-git-id-work")).unwrap(),
        private_key
    );
    assert_eq!(
        std::fs::read_to_string(ssh_dir.join("lum-git-id-work.pub")).unwrap(),
        public_key
    );
    assert_eq!(
        std::fs::read_to_string(ssh_dir.join("config")).unwrap(),
        "Host mine\n  User me\n"
    );
    let global_git_config = std::fs::read_to_string(home.path().join(".gitconfig")).unwrap();
    assert!(global_git_config.starts_with("[alias]\n  st = status\n\n# lum:git-id:begin\n"));
    assert!(global_git_config.contains(&format!("path = \"{root}/.gitconfig-lum-git-id-work\"")));
    let identity_config =
        std::fs::read_to_string(home.path().join(".gitconfig-lum-git-id-work")).unwrap();
    assert!(identity_config.contains("name = \"work\""));
    assert!(identity_config.contains(&format!(
        "sshCommand = \"ssh -i '{root}/.ssh/lum-git-id-work' -o IdentitiesOnly=yes\""
    )));

    lum_with_home(&home)
        .args(["git-id", "status"])
        .assert()
        .success()
        .stdout("work  active\n");
    let repo = folder.join("repo");
    git_with_home(&home)
        .arg("init")
        .arg(&repo)
        .assert()
        .success();
    git_with_home(&home)
        .args(["config", "--get", "user.email"])
        .current_dir(&repo)
        .assert()
        .success()
        .stdout("work@example.com\n");
    sync_command(&home)
        .args(["git-id", "sync"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Removed").not());
}

#[test]
fn safe_folder_nestings_route_to_the_most_specific_identity() {
    let home = TempDir::new().unwrap();
    let outer_folder = home.path().join("Work");
    let repo = outer_folder.join("Inner/repo");
    let inner_folder = outer_folder.join("Inner");

    let private_key = home.path().join(".ssh/lum-git-id-inner");
    let ssh_command = format!(
        "ssh -i '{}' -o IdentitiesOnly=yes\n",
        private_key.to_str().unwrap().replace('\\', "/")
    );
    for (outer, inner) in [
        (
            ssh_identity("outer", "a.invalid", &outer_folder),
            ssh_identity("inner", "a.invalid", &inner_folder),
        ),
        (
            http_identity("outer", "a.invalid", &outer_folder),
            ssh_identity("inner", "a.invalid", &inner_folder),
        ),
        (
            http_identity("outer", "a.invalid", &outer_folder),
            ssh_identity("inner", "b.invalid", &inner_folder),
        ),
    ] {
        write_identities(&home, &[outer, inner]);
        sync_command(&home)
            .args(["git-id", "sync"])
            .assert()
            .success()
            .stderr(predicates::str::contains("warning").not());
        git_with_home(&home)
            .arg("init")
            .arg(&repo)
            .assert()
            .success();
        git_with_home(&home)
            .args(["config", "--get", "core.sshCommand"])
            .current_dir(&repo)
            .assert()
            .success()
            .stdout(ssh_command.clone());
    }

    for inner_domain in ["a.invalid", "b.invalid"] {
        write_identities(
            &home,
            &[
                http_identity("outer", "a.invalid", &outer_folder),
                http_identity("inner", inner_domain, &inner_folder),
            ],
        );
        sync_command(&home)
            .args(["git-id", "sync"])
            .assert()
            .success()
            .stderr(predicates::str::contains("warning").not());
        git_with_home(&home)
            .args(["credential", "fill"])
            .current_dir(&repo)
            .write_stdin(format!("protocol=https\nhost={inner_domain}\n\n"))
            .assert()
            .success()
            .stdout(predicates::str::contains("username=inner-user\n"))
            .stdout(predicates::str::contains("password=inner-secret\n"));
    }
}

#[test]
fn a_refused_key_is_not_wired_into_global_files() {
    let home = TempDir::new().unwrap();
    write_identities(
        &home,
        &[
            ssh_identity("bad", "git.invalid", &home.path().join("Bad")),
            ssh_identity("good", "git.invalid", &home.path().join("Good")),
        ],
    );
    let ssh_dir = home.path().join(".ssh");
    std::fs::create_dir_all(&ssh_dir).unwrap();
    let foreign_key = "ssh-ed25519 FOREIGN someone@laptop\n";
    std::fs::write(ssh_dir.join("lum-git-id-bad"), "FOREIGN PRIVATE\n").unwrap();
    std::fs::write(ssh_dir.join("lum-git-id-bad.pub"), foreign_key).unwrap();

    sync_command(&home)
        .args(["git-id", "sync"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("refusing to touch unmarked key"))
        .stderr(predicates::str::contains("1 git identity sync failure(s)"));

    assert_eq!(
        std::fs::read_to_string(ssh_dir.join("lum-git-id-bad.pub")).unwrap(),
        foreign_key
    );
    assert!(ssh_dir.join("lum-git-id-bad").exists());
    let allowed_signers = std::fs::read_to_string(ssh_dir.join("allowed_signers")).unwrap();
    assert!(!allowed_signers.contains("FOREIGN"));
    assert!(allowed_signers.contains("good@example.com"));
    let global_git_config = std::fs::read_to_string(home.path().join(".gitconfig")).unwrap();
    assert!(!global_git_config.contains("lum-git-id-bad"));
    assert!(global_git_config.contains("lum-git-id-good"));
}

#[test]
fn a_refused_git_config_is_not_included() {
    let home = TempDir::new().unwrap();
    let folder = home.path().join("Work");
    write_identities(&home, &[ssh_identity("work", "git.invalid", &folder)]);
    let identity_config = home.path().join(".gitconfig-lum-git-id-work");
    let hand_written = "[user]\n  email = hand-written@example.com\n";
    std::fs::write(&identity_config, hand_written).unwrap();

    sync_command(&home)
        .args(["git-id", "sync"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("refusing to overwrite"));

    assert_eq!(
        std::fs::read_to_string(&identity_config).unwrap(),
        hand_written
    );
    let repo = folder.join("repo");
    git_with_home(&home)
        .arg("init")
        .arg(&repo)
        .assert()
        .success();
    git_with_home(&home)
        .args(["config", "--get", "user.email"])
        .current_dir(&repo)
        .assert()
        .code(1);
}

#[test]
fn status_reports_what_sync_would_change() {
    let home = TempDir::new().unwrap();
    let folder = home.path().join("Work");
    let identity = ssh_identity("work", "git.invalid", &folder);
    write_identities(&home, std::slice::from_ref(&identity));
    let status = |expected: &str| {
        lum_with_home(&home)
            .args(["git-id", "status"])
            .assert()
            .success()
            .stdout(format!("{expected}\n"));
    };

    status("work  needs sync (key missing)");
    sync_git_ids(&home);
    status("work  active");

    let mut changed = identity.clone();
    changed["email"] = "changed@example.com".into();
    write_identities(&home, &[changed]);
    status("work  needs sync (git config out of date)");
    write_identities(&home, std::slice::from_ref(&identity));
    status("work  active");

    std::fs::remove_file(home.path().join(".gitconfig-lum-git-id-work")).unwrap();
    status("work  needs sync (git config out of date)");
    sync_git_ids(&home);

    std::fs::write(home.path().join(".gitconfig"), "").unwrap();
    status("work  needs sync (folder not routed in ~/.gitconfig)");
    sync_git_ids(&home);

    git_with_home(&home)
        .args(["config", "--global", "user.email", "personal@example.com"])
        .assert()
        .success();
    status("work  needs sync (~/.gitconfig has settings after lum's section)");
    sync_git_ids(&home);
    status("work  active");
}

#[test]
fn where_reports_the_identity_git_actually_uses() {
    let home = TempDir::new().unwrap();
    let folder = home.path().join("Work");
    write_identities(&home, &[ssh_identity("work", "git.invalid", &folder)]);
    let repo = folder.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git_with_home(&home)
        .arg("init")
        .arg(&repo)
        .assert()
        .success();

    lum_with_home(&home)
        .args(["git-id", "where"])
        .current_dir(&repo)
        .assert()
        .failure()
        .stderr(predicates::str::contains("Git is not using identity work"))
        .stderr(predicates::str::contains("lum git-id sync"));

    sync_git_ids(&home);
    lum_with_home(&home)
        .args(["git-id", "where"])
        .current_dir(&repo)
        .assert()
        .success()
        .stdout(predicates::str::contains("Identity: work\n"));

    git_with_home(&home)
        .args(["config", "--global", "user.email", "personal@example.com"])
        .assert()
        .success();
    git_with_home(&home)
        .args(["config", "--get", "user.email"])
        .current_dir(&repo)
        .assert()
        .success()
        .stdout("personal@example.com\n");
    lum_with_home(&home)
        .args(["git-id", "where"])
        .current_dir(&repo)
        .assert()
        .failure()
        .stderr(predicates::str::contains("Git is not using identity work"))
        .stderr(predicates::str::contains(".gitconfig"));

    sync_git_ids(&home);
    lum_with_home(&home)
        .args(["git-id", "where"])
        .current_dir(&repo)
        .assert()
        .success()
        .stdout(predicates::str::contains("Email:    work@example.com\n"));

    git_with_home(&home)
        .args(["config", "user.email", "override@example.com"])
        .current_dir(&repo)
        .assert()
        .success();
    lum_with_home(&home)
        .args(["git-id", "where"])
        .current_dir(&repo)
        .assert()
        .failure()
        .stderr(predicates::str::contains("Git is not using identity work"))
        .stderr(predicates::str::contains(".git/config"));
}

#[test]
fn where_follows_git_for_linked_worktrees() {
    let home = TempDir::new().unwrap();
    let managed = home.path().join("Work");
    let unmanaged = home.path().join("Other");
    write_identities(&home, &[ssh_identity("work", "git.invalid", &managed)]);
    sync_git_ids(&home);
    let init_with_commit = |repo: &std::path::Path| {
        git_with_home(&home)
            .arg("init")
            .arg(repo)
            .assert()
            .success();
        git_with_home(&home)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--allow-empty",
                "-m",
                "first",
            ])
            .current_dir(repo)
            .assert()
            .success();
    };
    let add_worktree = |repo: &std::path::Path, worktree: &std::path::Path| {
        git_with_home(&home)
            .args(["worktree", "add"])
            .arg(worktree)
            .current_dir(repo)
            .assert()
            .success();
    };

    let outside_repo = unmanaged.join("repo");
    let inside_worktree = managed.join("worktree");
    init_with_commit(&outside_repo);
    add_worktree(&outside_repo, &inside_worktree);
    lum_with_home(&home)
        .args(["git-id", "where"])
        .current_dir(&inside_worktree)
        .assert()
        .failure()
        .stderr(predicates::str::contains("Git is not using identity work"))
        .stderr(predicates::str::contains("linked worktree"));

    let inside_repo = managed.join("repo");
    let outside_worktree = unmanaged.join("worktree");
    init_with_commit(&inside_repo);
    add_worktree(&inside_repo, &outside_worktree);
    lum_with_home(&home)
        .args(["git-id", "where"])
        .current_dir(&outside_worktree)
        .assert()
        .success()
        .stdout(predicates::str::contains("Identity: work\n"));

    lum_with_home(&home)
        .args(["git-id", "where"])
        .current_dir(&outside_repo)
        .assert()
        .failure()
        .stderr(predicates::str::contains("no git identity detected"));
}

#[test]
fn where_says_when_it_is_not_in_a_repository() {
    let home = TempDir::new().unwrap();
    let folder = home.path().join("Work");
    std::fs::create_dir_all(&folder).unwrap();
    write_identities(&home, &[ssh_identity("work", "git.invalid", &folder)]);

    lum_with_home(&home)
        .args(["git-id", "where"])
        .current_dir(&folder)
        .assert()
        .success()
        .stdout(predicates::str::contains("Identity: work\n"))
        .stderr(predicates::str::contains("Not in a Git repository"));
}

#[test]
fn pubkey_prints_only_the_key_and_paths_lists_the_files_sync_writes() {
    let home = TempDir::new().unwrap();
    write_identities(
        &home,
        &[ssh_identity(
            "work",
            "git.invalid",
            &home.path().join("Work"),
        )],
    );

    lum_with_home(&home)
        .args(["git-id", "pubkey", "work"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("run lum git-id sync"));

    sync_git_ids(&home);
    let public_key = std::fs::read_to_string(home.path().join(".ssh/lum-git-id-work.pub")).unwrap();
    lum_with_home(&home)
        .args(["git-id", "pubkey", "work"])
        .assert()
        .success()
        .stdout(public_key);
    lum_with_home(&home)
        .args(["git-id", "paths"])
        .assert()
        .success()
        .stdout(predicates::str::contains(".gitconfig"))
        .stdout(predicates::str::contains(".ssh/allowed_signers"))
        .stdout(predicates::str::contains(".ssh/config").not());
}

#[cfg(unix)]
#[test]
fn real_keys_sign_and_verify_commits_in_managed_folders() {
    let has_ssh_keygen = std::process::Command::new("ssh-keygen")
        .arg("-h")
        .output()
        .is_ok();
    if !has_ssh_keygen {
        return;
    }
    let home = TempDir::new().unwrap();
    let folder = home.path().join("Work");
    write_identities(&home, &[ssh_identity("work", "git.invalid", &folder)]);

    lum_with_home(&home)
        .args(["git-id", "sync"])
        .assert()
        .success();
    lum_with_home(&home)
        .args(["git-id", "status"])
        .assert()
        .success()
        .stdout("work  active\n");

    let repo = folder.join("repo");
    git_with_home(&home)
        .arg("init")
        .arg(&repo)
        .assert()
        .success();
    git_with_home(&home)
        .env_remove("SSH_AUTH_SOCK")
        .args(["commit", "--allow-empty", "-m", "signed"])
        .current_dir(&repo)
        .assert()
        .success();
    git_with_home(&home)
        .env_remove("SSH_AUTH_SOCK")
        .args(["log", "-1", "--format=%an <%ae> %G?"])
        .current_dir(&repo)
        .assert()
        .success()
        .stdout("work <work@example.com> G\n");
}
