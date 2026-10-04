use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use tempfile::TempDir;

fn lum_with_home(home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("lum").unwrap();
    cmd.env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("XDG_DATA_HOME", home.path().join(".local/share"))
        .env("XDG_STATE_HOME", home.path().join(".local/state"));
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

fn sync_git_ids(home: &TempDir) {
    sync_command(home)
        .args(["git-id", "sync"])
        .assert()
        .success();
}

fn sync_command(home: &TempDir) -> Command {
    let mut command = lum_with_home(home);
    #[cfg(unix)]
    {
        let bin = home.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let fake_ssh_keygen = bin.join("ssh-keygen");
        std::fs::write(
            &fake_ssh_keygen,
            "#!/bin/sh\nif [ \"$1\" = \"-h\" ]; then exit 0; fi\nkey=\"\"\ncomment=\"\"\nwhile [ $# -gt 0 ]; do\n  case \"$1\" in\n    -f) shift; key=\"$1\" ;;\n    -C) shift; comment=\"$1\" ;;\n  esac\n  shift\ndone\nprintf 'PRIVATE %s\n' \"$comment\" > \"$key\"\nprintf 'ssh-ed25519 TESTKEY %s\n' \"$comment\" > \"$key.pub\"\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&fake_ssh_keygen).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&fake_ssh_keygen, permissions).unwrap();

        let mut paths = vec![bin];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        command.env("PATH", std::env::join_paths(paths).unwrap());
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
    let ssh_config = std::fs::read_to_string(home.path().join(".ssh/config")).unwrap();
    assert!(ssh_config.contains("Host existing\n  User git\n\n# lum:git-id:begin"));
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

    let ssh_config = std::fs::read_to_string(home.path().join(".ssh/config")).unwrap();
    assert!(!ssh_config.contains("gitlab.dev.circletouch.eu"));
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
    let ssh_config = std::fs::read_to_string(home.path().join(".ssh/config")).unwrap();
    assert!(ssh_config.contains("Host gitlab.dev.circletouch.eu"));
}
