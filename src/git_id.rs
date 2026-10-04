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
    is_lum_managed_file, replace_marked_section,
};
pub use config::config_path;
use config::{
    Authentication, Identity, allowed_signers_path, data_dir, detect_identity, git_path,
    identity_git_config_path, identity_private_key_path, identity_public_key_path, load_config,
};

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
    let identity = detect_identity(&identities, &cwd)
        .ok_or_else(|| anyhow::anyhow!("no git identity detected for {}", cwd.display()))?;
    println!("Identity: {}", identity.name);
    println!("Author:   {}", identity.author_name);
    println!("Email:    {}", identity.email);
    println!("Domain:   {}", identity.domain);
    Ok(())
}

fn sync() -> Result<()> {
    let path = config_path()?;
    config::harden_config_permissions(&path)?;
    let identities = load_config()?;
    ensure_ssh_keygen_on_path()?;
    cleanup_old_backups()?;

    let mut failures = Vec::new();
    for identity in &identities {
        if let Err(error) = sync_identity(identity) {
            failures.push(format!("{}: {error:#}", identity.name));
            eprintln!("{}: {error:#}", identity.name);
        }
    }

    write_global_git_config(&identities)?;
    write_ssh_config(&identities)?;
    write_allowed_signers(&identities)?;
    cleanup_orphan_identity_configs(&identities, identity_git_config_path)?;
    cleanup_orphan_key_pairs(&identities)?;

    if !failures.is_empty() {
        anyhow::bail!("{} git identity sync failure(s)", failures.len());
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
    let public_key = git_path(&identity_public_key_path(identity)?);
    let allowed_signers = git_path(&allowed_signers_path()?);
    let mut content = format!(
        "# lum:git-id:managed identity={}\n\n[user]\n  name = {}\n  email = {}\n  signingkey = {}\n\n[commit]\n  gpgsign = true\n\n[gpg]\n  format = ssh\n\n[gpg \"ssh\"]\n  allowedSignersFile = {}\n",
        identity.name, identity.author_name, identity.email, public_key, allowed_signers,
    );
    match &identity.authentication {
        Authentication::Ssh => {
            let private_key = git_path(&identity_private_key_path(identity)?);
            content.push_str(&format!(
                "\n[core]\n  sshCommand = \"ssh -i {} -o IdentitiesOnly=yes\"\n\n[url \"ssh://git@{}/\"]\n  insteadOf = https://{}/\n",
                private_key, identity.domain, identity.domain
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
    std::fs::write(path, content)?;
    Ok(())
}

fn write_global_git_config(identities: &[Identity]) -> Result<()> {
    let path = crate::paths::home_path(".gitconfig")?;
    let mut entries: Vec<_> = identities
        .iter()
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

    let mut section = String::from("# lum:git-id:begin\n");
    for (identity, folder) in entries {
        let expanded = expand_path(folder);
        let resolved = dunce::canonicalize(&expanded).unwrap_or(expanded);
        let folder_path = if cfg!(windows) {
            git_path(&resolved)
        } else {
            resolved.to_string_lossy().into_owned()
        };
        let mut folder_path = globset::escape(&folder_path.replace('\\', r"\\"));
        if folder_path.contains('\n') {
            anyhow::bail!("managed folder path contains a newline: {resolved:?}");
        }
        if !folder_path.ends_with('/') {
            folder_path.push('/');
        }
        let folder_path = folder_path.replace('\\', "\\\\").replace('"', "\\\"");
        section.push_str(&format!(
            "[includeIf \"gitdir:{}\"]\n  path = {}\n\n",
            folder_path,
            git_path(&identity_git_config_path(identity)?)
        ));
    }
    section.push_str("# lum:git-id:end\n");
    replace_marked_section(&path, "# lum:git-id:begin", "# lum:git-id:end", &section)
}

fn write_ssh_config(identities: &[Identity]) -> Result<()> {
    let path = crate::paths::home_path(".ssh/config")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut seen = std::collections::HashSet::new();
    let mut section = String::from("# lum:git-id:begin\n");
    for identity in identities {
        if !matches!(&identity.authentication, Authentication::Ssh) {
            continue;
        }
        if seen.insert(identity.domain.clone()) {
            section.push_str(&format!(
                "Host {}\n  HostName {}\n  User git\n  IdentityFile {}\n  IdentitiesOnly yes\n\n",
                identity.domain,
                identity.domain,
                git_path(&identity_private_key_path(identity)?)
            ));
        }
    }
    section.push_str("# lum:git-id:end\n");
    replace_marked_section(&path, "# lum:git-id:begin", "# lum:git-id:end", &section)
}

fn write_allowed_signers(identities: &[Identity]) -> Result<()> {
    let path = allowed_signers_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut section = String::from("# lum:git-id:begin\n");
    for identity in identities {
        let public_key = identity_public_key_path(identity)?;
        if let Ok(content) = std::fs::read_to_string(public_key) {
            let fields: Vec<_> = content.split_whitespace().collect();
            if fields.len() >= 2 {
                section.push_str(&format!("{} {} {}\n", identity.email, fields[0], fields[1]));
            }
        }
    }
    section.push_str("# lum:git-id:end\n");
    replace_marked_section(&path, "# lum:git-id:begin", "# lum:git-id:end", &section)
}

fn status() -> Result<()> {
    for identity in load_config()? {
        let key = identity_public_key_path(&identity)?;
        let status = if key.exists() { "active" } else { "needs sync" };
        println!("{}  {}", identity.name, status);
    }
    Ok(())
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
            .join(".ssh/config")
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
