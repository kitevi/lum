use anyhow::{Context, Result};

use crate::cli::GitIdCommand;

#[path = "git_id/artifacts.rs"]
mod artifacts;
#[path = "git_id/config.rs"]
mod config;
#[path = "git_id/credential.rs"]
mod credential;

use crate::paths::expand_path;
use artifacts::{
    cleanup_old_backups, cleanup_orphan_identity_configs, cleanup_orphan_key_pairs,
    has_settings_after_section, is_lum_managed_file, read_optional, remove_marked_section,
    replace_marked_section,
};
pub use config::config_path;
use config::{
    Authentication, Identity, allowed_signers_path, data_dir, detect_identity,
    folder_nesting_warnings, git_path, identity_git_config_path, identity_private_key_path,
    identity_public_key_path, load_config,
};

const SECTION_BEGIN: &str = "# lum:git-id:begin";
const SECTION_END: &str = "# lum:git-id:end";

pub fn run_credential_helper(route_id: &str, operation: &str) -> Result<()> {
    credential::run(route_id, operation)
}

pub fn run(command: GitIdCommand) -> Result<()> {
    match command {
        GitIdCommand::ConfigPath => {
            println!("{}", config_path()?.display());
            Ok(())
        }
        GitIdCommand::Init => init(),
        GitIdCommand::Where => where_am_i(),
        GitIdCommand::Sync => sync(),
        GitIdCommand::Status => status(),
        GitIdCommand::Info { identity } => info(&identity),
        GitIdCommand::Pubkey { identity } => pubkey(&identity),
        GitIdCommand::Paths => paths(),
    }
}

fn init() -> Result<()> {
    let path = config_path()?;
    if path.exists() {
        println!("{} already exists, not overwriting", path.display());
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let sample = serde_json::json!({
        "identities": [{
            "name": "github-work",
            "author_name": "Jane Doe",
            "email": "jane@company.com",
            "domain": "github.com",
            "folders": ["~/Work/Github"]
        }]
    });
    config::create_private_config(&path, serde_json::to_string_pretty(&sample)?.as_bytes())?;
    println!("Created {} with a sample git identity", path.display());
    Ok(())
}

fn where_am_i() -> Result<()> {
    let identities = load_config()?;
    let cwd = std::env::current_dir()?;
    let expected = detect_identity(&identities, &cwd);
    let undetected = || anyhow::anyhow!("no git identity detected for {}", cwd.display());
    let identity = match git_email_origin() {
        None => {
            let identity = expected.ok_or_else(undetected)?;
            eprintln!(
                "Not in a Git repository; showing the identity for repositories created here."
            );
            identity
        }
        Some(origin) => {
            let active = origin
                .as_deref()
                .and_then(identity_name_from_origin)
                .and_then(|name| identities.iter().find(|identity| identity.name == name));
            match (active, expected, origin) {
                (Some(identity), _, _) => identity,
                (None, None, _) => return Err(undetected()),
                (None, Some(expected), Some(origin)) => anyhow::bail!(
                    "Git is not using identity {} here: user.email comes from {origin}. Settings in the repository or after lum's section in ~/.gitconfig take precedence; lum git-id sync moves that section back to the end",
                    expected.name
                ),
                (None, Some(expected), None) => anyhow::bail!(
                    "Git is not using identity {} here. Run lum git-id sync if you have not; otherwise this is a linked worktree of a repository outside the managed folder, and Git goes by the location of the .git directory",
                    expected.name
                ),
            }
        }
    };
    println!("Identity: {}", identity.name);
    println!("Author:   {}", identity.author_name);
    println!("Email:    {}", identity.email);
    println!("Domain:   {}", identity.domain);
    Ok(())
}

/// Where Git gets `user.email` in the current directory. The outer `None` means this is
/// not a repository or Git is unavailable; the inner `None` means `user.email` is unset.
fn git_email_origin() -> Option<Option<String>> {
    let in_repository = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .is_ok_and(|output| output.status.success());
    if !in_repository {
        return None;
    }
    let output = std::process::Command::new("git")
        .args(["config", "--null", "--show-origin", "--get", "user.email"])
        .output()
        .ok()?;
    if !output.status.success() {
        return Some(None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Some(stdout.split('\0').next().map(str::to_owned))
}

fn identity_name_from_origin(origin: &str) -> Option<&str> {
    std::path::Path::new(origin.strip_prefix("file:")?)
        .file_name()?
        .to_str()?
        .strip_prefix(".gitconfig-lum-git-id-")
}

fn sync() -> Result<()> {
    let path = config_path()?;
    crate::private_file::harden(&path)?;
    let identities = load_config()?;
    for warning in folder_nesting_warnings(&identities) {
        eprintln!("warning: {warning}");
    }
    ensure_ssh_keygen_on_path()?;
    cleanup_old_backups()?;

    let mut synced = Vec::new();
    for identity in &identities {
        match sync_identity(identity) {
            Ok(()) => synced.push(identity),
            Err(error) => eprintln!("{}: {error:#}", identity.name),
        }
    }

    write_global_git_config(&synced)?;
    // Older versions wrote a Host block here; see ADR 0021.
    let ssh_config = crate::paths::home_path(".ssh/config")?;
    if let Some(removed) = remove_marked_section(&ssh_config, SECTION_BEGIN, SECTION_END)? {
        println!(
            "Removed lum's Host section from {}. SSH outside managed folders now uses your own settings. To keep using these keys there, add this to the file yourself:\n\n{}\n",
            ssh_config.display(),
            removed.trim_end()
        );
    }
    write_allowed_signers(&synced)?;
    cleanup_orphan_identity_configs(&identities, identity_git_config_path)?;
    cleanup_orphan_key_pairs(&identities)?;

    let failures = identities.len() - synced.len();
    if failures > 0 {
        anyhow::bail!("{failures} git identity sync failure(s)");
    }
    Ok(())
}

fn ensure_ssh_keygen_on_path() -> Result<()> {
    match std::process::Command::new("ssh-keygen").arg("-h").output() {
        Ok(_) => Ok(()),
        Err(_) => anyhow::bail!("ssh-keygen executable not found on PATH"),
    }
}

fn sync_identity(identity: &Identity) -> Result<()> {
    for folder in &identity.folders {
        std::fs::create_dir_all(expand_path(folder))?;
    }
    ensure_key_pair(identity)?;
    write_identity_git_config(identity)?;
    println!("{}", identity.name);
    Ok(())
}

fn ensure_key_pair(identity: &Identity) -> Result<()> {
    let private_key = identity_private_key_path(identity)?;
    let public_key = identity_public_key_path(identity)?;

    if private_key.exists() || public_key.exists() {
        ensure_lum_managed_key(identity, &public_key)?;
        return Ok(());
    }

    if let Some(parent) = private_key.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let comment = format!("{} [lum:git-id identity={}]", identity.email, identity.name);
    let output = std::process::Command::new("ssh-keygen")
        .args(["-t", "ed25519", "-C", &comment, "-f"])
        .arg(&private_key)
        .args(["-N", ""])
        .output()
        .with_context(|| "running ssh-keygen")?;
    if !output.status.success() {
        anyhow::bail!(
            "ssh-keygen failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    ensure_lum_managed_key(identity, &public_key)?;
    println!("  public key: {}", public_key.display());
    Ok(())
}

fn ensure_lum_managed_key(identity: &Identity, public_key: &std::path::Path) -> Result<()> {
    let marker = format!("[lum:git-id identity={}]", identity.name);
    let content = std::fs::read_to_string(public_key)
        .with_context(|| format!("reading public key {}", public_key.display()))?;
    if !content.contains(&marker) {
        anyhow::bail!(
            "refusing to touch unmarked key at {} (missing {})",
            public_key.display(),
            marker
        );
    }
    Ok(())
}

fn write_identity_git_config(identity: &Identity) -> Result<()> {
    let path = identity_git_config_path(identity)?;
    if path.exists()
        && !is_lum_managed_file(
            &path,
            &format!("# lum:git-id:managed identity={}", identity.name),
        )?
    {
        anyhow::bail!(
            "refusing to overwrite unmarked git config at {}",
            path.display()
        );
    }
    std::fs::write(path, identity_git_config(identity)?)?;
    Ok(())
}

fn identity_git_config(identity: &Identity) -> Result<String> {
    let public_key = git_path(&identity_public_key_path(identity)?);
    let allowed_signers = git_path(&allowed_signers_path()?);
    let mut content = format!(
        "# lum:git-id:managed identity={}\n\n[user]\n  name = {}\n  email = {}\n  signingkey = {}\n\n[commit]\n  gpgsign = true\n\n[gpg]\n  format = ssh\n\n[gpg \"ssh\"]\n  allowedSignersFile = {}\n",
        identity.name,
        credential::git_config_value(&identity.author_name),
        credential::git_config_value(&identity.email),
        credential::git_config_value(&public_key),
        credential::git_config_value(&allowed_signers),
    );
    match &identity.authentication {
        Authentication::Ssh => {
            let private_key = git_path(&identity_private_key_path(identity)?);
            let ssh_command = format!(
                "ssh -i {} -o IdentitiesOnly=yes",
                credential::shell_quote(&private_key)
            );
            content.push_str(&format!(
                "\n[core]\n  sshCommand = {}\n\n[url \"ssh://git@{}/\"]\n  insteadOf = https://{}/\n",
                credential::git_config_value(&ssh_command),
                identity.domain,
                identity.domain
            ));
        }
        Authentication::HttpBasic {
            scheme, username, ..
        } => {
            let helper = credential::helper_definition(&identity.name)?;
            let username = credential::git_config_value(username);
            content.push_str(&format!(
                "\n[credential \"{}://{}\"]\n  username = {}\n  useHttpPath = false\n  helper =\n  helper = {}\n",
                scheme.as_str(), identity.domain, username, helper
            ));
        }
    }
    Ok(content)
}

fn write_global_git_config(identities: &[&Identity]) -> Result<()> {
    let path = crate::paths::home_path(".gitconfig")?;
    let mut entries: Vec<_> = identities
        .iter()
        .copied()
        .flat_map(|identity| {
            identity
                .folders
                .iter()
                .map(move |folder| (identity, folder))
        })
        .collect();
    entries.sort_by_key(|(_, folder)| {
        crate::paths::normalize_path(&expand_path(folder))
            .components()
            .count()
    });

    let mut section = format!("{SECTION_BEGIN}\n");
    for (identity, folder) in entries {
        section.push_str(&include_entry(identity, folder)?);
    }
    section.push_str(&format!("{SECTION_END}\n"));
    replace_marked_section(&path, SECTION_BEGIN, SECTION_END, &section)
}

fn include_entry(identity: &Identity, folder: &str) -> Result<String> {
    let expanded = expand_path(folder);
    let resolved = dunce::canonicalize(&expanded).unwrap_or(expanded);
    let mut folder_path = globset::escape(&git_path(&resolved).replace('\\', r"\\"));
    if folder_path.contains('\n') {
        anyhow::bail!("managed folder path contains a newline: {resolved:?}");
    }
    if !folder_path.ends_with('/') {
        folder_path.push('/');
    }
    let folder_path = folder_path.replace('\\', "\\\\").replace('"', "\\\"");
    Ok(format!(
        "[includeIf \"gitdir:{}\"]\n  path = {}\n\n",
        folder_path,
        credential::git_config_value(&git_path(&identity_git_config_path(identity)?))
    ))
}

fn write_allowed_signers(identities: &[&Identity]) -> Result<()> {
    let path = allowed_signers_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut section = format!("{SECTION_BEGIN}\n");
    for identity in identities.iter().copied() {
        let public_key = identity_public_key_path(identity)?;
        if let Ok(content) = std::fs::read_to_string(public_key) {
            let fields: Vec<_> = content.split_whitespace().collect();
            if fields.len() >= 2 {
                section.push_str(&format!("{} {} {}\n", identity.email, fields[0], fields[1]));
            }
        }
    }
    section.push_str(&format!("{SECTION_END}\n"));
    replace_marked_section(&path, SECTION_BEGIN, SECTION_END, &section)
}

fn status() -> Result<()> {
    let global = read_optional(&crate::paths::home_path(".gitconfig")?)?;
    for identity in load_config()? {
        match identity_sync_problem(&identity, &global)? {
            None => println!("{}  active", identity.name),
            Some(problem) => println!("{}  needs sync ({problem})", identity.name),
        }
    }
    Ok(())
}

/// Compares an identity's artifacts with what sync would write.
fn identity_sync_problem(identity: &Identity, global: &str) -> Result<Option<&'static str>> {
    if !identity_private_key_path(identity)?.exists()
        || !identity_public_key_path(identity)?.exists()
    {
        return Ok(Some("key missing"));
    }
    if read_optional(&identity_git_config_path(identity)?)? != identity_git_config(identity)? {
        return Ok(Some("git config out of date"));
    }
    for folder in &identity.folders {
        if !global.contains(&include_entry(identity, folder)?) {
            return Ok(Some("folder not routed in ~/.gitconfig"));
        }
    }
    if has_settings_after_section(global, SECTION_END) {
        return Ok(Some("~/.gitconfig has settings after lum's section"));
    }
    Ok(None)
}

fn info(name: &str) -> Result<()> {
    let identities = load_config()?;
    let identity = identities
        .iter()
        .find(|identity| identity.name == name)
        .ok_or_else(|| anyhow::anyhow!("identity not found: {name}"))?;
    println!("Identity: {}", identity.name);
    println!("Author:   {}", identity.author_name);
    println!("Email:    {}", identity.email);
    println!("Domain:   {}", identity.domain);
    match &identity.authentication {
        Authentication::Ssh => println!("Authentication: SSH"),
        Authentication::HttpBasic {
            scheme, username, ..
        } => {
            println!(
                "Authentication: HTTP Basic ({}://{})",
                scheme.as_str(),
                identity.domain
            );
            println!("Username: {}", username);
            println!("Password: configured");
        }
    }
    println!(
        "Git config: {}",
        identity_git_config_path(identity)?.display()
    );
    println!(
        "Public key: {}",
        identity_public_key_path(identity)?.display()
    );
    Ok(())
}

fn pubkey(name: &str) -> Result<()> {
    let identities = load_config()?;
    let identity = identities
        .iter()
        .find(|identity| identity.name == name)
        .ok_or_else(|| anyhow::anyhow!("identity not found: {name}"))?;
    let path = identity_public_key_path(identity)?;
    let content = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "reading public key {} (run lum git-id sync)",
            path.display()
        )
    })?;
    print!("{}", content);
    Ok(())
}

fn paths() -> Result<()> {
    let identities = if config_path()?.exists() {
        load_config()?
    } else {
        vec![]
    };
    println!("Config:\n  {}", config_path()?.display());
    println!("Global files touched:");
    println!(
        "  {}",
        crate::paths::home_dir()
            .unwrap_or_default()
            .join(".gitconfig")
            .display()
    );
    println!(
        "  {}",
        crate::paths::home_dir()
            .unwrap_or_default()
            .join(".ssh/allowed_signers")
            .display()
    );
    println!("Backups:\n  {}", data_dir()?.join("backups").display());
    if !identities.is_empty() {
        println!("Identities:");
        for identity in identities {
            println!("  {}", identity.name);
            println!(
                "    git config: {}",
                identity_git_config_path(&identity)?.display()
            );
            println!(
                "    private key: {}",
                identity_private_key_path(&identity)?.display()
            );
            println!(
                "    public key: {}",
                identity_public_key_path(&identity)?.display()
            );
            for folder in identity.folders {
                println!("    folder: {}", folder);
            }
        }
    }
    Ok(())
}
