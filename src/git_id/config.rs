use anyhow::{Context, Result};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub name: String,
    pub author_name: String,
    pub email: String,
    pub domain: String,
    pub folders: Vec<String>,
    #[serde(default)]
    pub authentication: Authentication,
}

#[derive(Debug, Default, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Authentication {
    #[default]
    Ssh,
    HttpBasic {
        scheme: HttpScheme,
        username: String,
        password: SecretString,
        #[serde(default)]
        allow_insecure_http: bool,
    },
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HttpScheme {
    Http,
    Https,
}

impl HttpScheme {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitIdentitiesConfig {
    pub identities: Vec<Identity>,
}

/// The fields the credential helper needs. It parses only these, so a problem elsewhere
/// in the config cannot block authentication during ordinary Git operations.
#[derive(Debug, Deserialize)]
pub struct CredentialIdentity {
    pub name: String,
    pub domain: String,
    #[serde(default)]
    pub authentication: Authentication,
}

#[derive(Debug, Deserialize)]
struct CredentialConfig {
    identities: Vec<CredentialIdentity>,
}

pub fn config_path() -> Result<PathBuf> {
    crate::paths::git_id_config_file()
}

pub fn data_dir() -> Result<PathBuf> {
    crate::paths::git_id_data_dir()
}

pub fn create_private_config(path: &Path, content: &[u8]) -> Result<()> {
    use std::io::Write as _;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("creating {}", path.display()))?;
    file.write_all(content)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub fn ensure_private_config_permissions(path: &Path) -> Result<()> {
    let _ = path;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path)?.permissions().mode();
        if mode & 0o077 != 0 {
            anyhow::bail!("git identity config permissions are not private; run lum git-id sync");
        }
    }
    Ok(())
}

pub fn load_config() -> Result<Vec<Identity>> {
    let path = config_path()?;
    let content =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let config: GitIdentitiesConfig =
        serde_json::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
    let mut identities = config.identities;
    for identity in &mut identities {
        identity.domain.make_ascii_lowercase();
    }
    validate(&identities)?;
    Ok(identities)
}

pub fn load_credential_identities() -> Result<Vec<CredentialIdentity>> {
    let path = config_path()?;
    let content =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let config: CredentialConfig =
        serde_json::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
    for identity in &config.identities {
        validate_authentication(&identity.name, &identity.authentication)?;
    }
    Ok(config.identities)
}

fn validate(identities: &[Identity]) -> Result<()> {
    let mut names = HashSet::new();
    let mut folders = HashSet::new();
    let mut email_domains = HashSet::new();
    let mut author_domains = HashSet::new();

    for identity in identities {
        if identity.name.trim().is_empty() {
            anyhow::bail!("identity name must not be empty");
        }
        validate_identity_name(&identity.name)?;
        if !names.insert(identity.name.clone()) {
            anyhow::bail!("duplicate identity name: {}", identity.name);
        }
        if identity.author_name.trim().is_empty() {
            anyhow::bail!("identity {}: author_name must not be empty", identity.name);
        }
        if identity.email.trim().is_empty() {
            anyhow::bail!("identity {}: email must not be empty", identity.name);
        }
        if identity.domain.trim().is_empty() {
            anyhow::bail!("identity {}: domain must not be empty", identity.name);
        }
        validate_protocol_scalar(&identity.name, "author_name", &identity.author_name)?;
        validate_protocol_scalar(&identity.name, "email", &identity.email)?;
        validate_email(identity)?;
        validate_domain(identity)?;
        validate_authentication(&identity.name, &identity.authentication)?;
        if identity.folders.is_empty() {
            anyhow::bail!(
                "identity {}: at least one folder is required",
                identity.name
            );
        }
        let email_domain = (identity.email.clone(), identity.domain.clone());
        if !email_domains.insert(email_domain) {
            anyhow::bail!(
                "duplicate email+domain: {} on {}",
                identity.email,
                identity.domain
            );
        }
        let author_domain = (identity.author_name.clone(), identity.domain.clone());
        if !author_domains.insert(author_domain) {
            anyhow::bail!(
                "duplicate author_name+domain: {} on {}",
                identity.author_name,
                identity.domain
            );
        }
        for folder in &identity.folders {
            if folder.trim().is_empty() {
                anyhow::bail!("identity {}: folder must not be empty", identity.name);
            }
            let expanded = crate::paths::expand_path(folder);
            if !expanded.is_absolute() {
                anyhow::bail!(
                    "identity {}: folder {} must be an absolute path or start with ~/",
                    identity.name,
                    folder
                );
            }
            let normalized = crate::paths::normalize_path(&expanded);
            if !folders.insert(normalized) {
                anyhow::bail!("duplicate managed folder: {}", folder);
            }
        }
    }
    validate_folder_nesting(identities)
}

/// Names become file names; quoting keeps every other character safe in generated config.
fn validate_identity_name(name: &str) -> Result<()> {
    if name
        .chars()
        .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        anyhow::bail!(
            "identity name {name:?} must not contain path separators or control characters"
        );
    }
    if name.ends_with(".pub") {
        anyhow::bail!("identity name {name:?} must not end in .pub");
    }
    Ok(())
}

/// Whitespace, quotes, and commas change the meaning of an allowed_signers line.
fn validate_email(identity: &Identity) -> Result<()> {
    if identity
        .email
        .chars()
        .any(|character| character.is_whitespace() || matches!(character, '"' | ','))
    {
        anyhow::bail!(
            "identity {}: email must not contain whitespace, quotes, or commas",
            identity.name
        );
    }
    Ok(())
}

/// A folder inside an SSH identity's folder that belongs to an identity on another route.
/// Git merges every matching include, and an inner config cannot unset an inherited
/// `insteadOf`, so the outer identity's SSH command and HTTPS rewrite still apply there.
struct NestedFolder<'a> {
    outer: &'a Identity,
    outer_folder: PathBuf,
    inner: &'a Identity,
    inner_folder: PathBuf,
}

impl NestedFolder<'_> {
    fn describe(&self) -> String {
        format!(
            "identity {}: folder {} is inside folder {} of SSH identity {}, whose SSH command and HTTPS rewrite still apply there",
            self.inner.name,
            self.inner_folder.display(),
            self.outer_folder.display(),
            self.outer.name
        )
    }
}

fn folders_nested_under_ssh(identities: &[Identity]) -> Vec<NestedFolder<'_>> {
    let folders: Vec<_> = identities
        .iter()
        .flat_map(|identity| {
            identity.folders.iter().map(move |folder| {
                let expanded = crate::paths::expand_path(folder);
                (identity, crate::paths::normalize_path(&expanded))
            })
        })
        .collect();
    let mut nested = Vec::new();
    for (outer, outer_folder) in &folders {
        if !matches!(outer.authentication, Authentication::Ssh) {
            continue;
        }
        for (inner, inner_folder) in &folders {
            let same_route =
                matches!(inner.authentication, Authentication::Ssh) && inner.domain == outer.domain;
            if !same_route && is_path_prefix(outer_folder, inner_folder) {
                nested.push(NestedFolder {
                    outer,
                    outer_folder: outer_folder.clone(),
                    inner,
                    inner_folder: inner_folder.clone(),
                });
            }
        }
    }
    nested
}

/// The outer rewrite turns every HTTPS URL for its domain into SSH, so an inner HTTPS
/// identity on that domain could never use its credentials.
fn validate_folder_nesting(identities: &[Identity]) -> Result<()> {
    for nested in folders_nested_under_ssh(identities) {
        if nested.inner.domain == nested.outer.domain
            && matches!(
                nested.inner.authentication,
                Authentication::HttpBasic {
                    scheme: HttpScheme::Https,
                    ..
                }
            )
        {
            anyhow::bail!(
                "{}, so its credentials would never be used",
                nested.describe()
            );
        }
    }
    Ok(())
}

/// Nestings that still work but where the outer identity's routing leaks into the folder.
pub fn folder_nesting_warnings(identities: &[Identity]) -> Vec<String> {
    folders_nested_under_ssh(identities)
        .iter()
        .map(NestedFolder::describe)
        .collect()
}

fn validate_authentication(name: &str, authentication: &Authentication) -> Result<()> {
    let Authentication::HttpBasic {
        scheme,
        username,
        password,
        allow_insecure_http,
    } = authentication
    else {
        return Ok(());
    };

    if username.trim().is_empty() {
        anyhow::bail!("identity {name}: authentication username must not be empty");
    }
    validate_protocol_scalar(name, "authentication username", username)?;
    let password = password.expose_secret();
    if password.is_empty() {
        anyhow::bail!("identity {name}: authentication password must not be empty");
    }
    if password
        .bytes()
        .any(|byte| matches!(byte, b'\0' | b'\n' | b'\r'))
    {
        anyhow::bail!("identity {name}: authentication password must not contain NUL, CR, or LF");
    }

    match (scheme, allow_insecure_http) {
        (HttpScheme::Http, false) => anyhow::bail!(
            "identity {name}: plain HTTP exposes credentials on the network; set allow_insecure_http to true to acknowledge this"
        ),
        (HttpScheme::Https, true) => {
            anyhow::bail!("identity {name}: allow_insecure_http is only valid with the http scheme")
        }
        _ => Ok(()),
    }
}

/// Domains appear unquoted in generated section names and URLs. The URL parser forgives
/// input that is unsafe there, such as embedded newlines, so check the characters first.
fn validate_domain(identity: &Identity) -> Result<()> {
    let has_safe_characters = identity.domain.chars().all(|character| {
        character.is_alphanumeric() || matches!(character, '.' | '_' | '-' | ':' | '[' | ']')
    });
    if !has_safe_characters
        || identity.domain.ends_with(':')
        || url::Url::parse(&format!("https://{}/", identity.domain)).is_err()
    {
        anyhow::bail!(
            "identity {}: domain must be a host with an optional port, without a scheme or path",
            identity.name
        );
    }
    Ok(())
}

fn validate_protocol_scalar(name: &str, field: &str, value: &str) -> Result<()> {
    if value
        .bytes()
        .any(|byte| matches!(byte, b'\0' | b'\n' | b'\r'))
    {
        anyhow::bail!("identity {name}: {field} must not contain NUL, CR, or LF");
    }
    Ok(())
}

pub fn detect_identity<'a>(
    identities: &'a [Identity],
    dir: &std::path::Path,
) -> Option<&'a Identity> {
    let dir = crate::paths::normalize_path(dir);
    identities
        .iter()
        .filter_map(|identity| {
            identity
                .folders
                .iter()
                .map(|folder| crate::paths::normalize_path(&crate::paths::expand_path(folder)))
                .filter(|folder| is_path_prefix(folder, &dir))
                .map(|folder| (folder.components().count(), identity))
                .max_by_key(|(len, _)| *len)
        })
        .max_by_key(|(len, _)| *len)
        .map(|(_, identity)| identity)
}

fn is_path_prefix(prefix: &std::path::Path, path: &std::path::Path) -> bool {
    path == prefix || path.starts_with(prefix)
}

pub fn identity_private_key_path(identity: &Identity) -> Result<PathBuf> {
    Ok(crate::paths::home_dir()?
        .join(".ssh")
        .join(format!("lum-git-id-{}", identity.name)))
}

pub fn identity_public_key_path(identity: &Identity) -> Result<PathBuf> {
    Ok(crate::paths::home_dir()?
        .join(".ssh")
        .join(format!("lum-git-id-{}.pub", identity.name)))
}

pub fn identity_git_config_path(identity: &Identity) -> Result<PathBuf> {
    Ok(crate::paths::home_dir()?.join(format!(".gitconfig-lum-git-id-{}", identity.name)))
}

pub fn allowed_signers_path() -> Result<PathBuf> {
    crate::paths::home_path(".ssh/allowed_signers")
}

pub fn git_path(path: &std::path::Path) -> String {
    let path = path.to_string_lossy();
    if cfg!(windows) {
        windows_git_path(&path)
    } else {
        path.into_owned()
    }
}

fn windows_git_path(path: &str) -> String {
    let path = path.replace('\\', "/");
    if let Some(network_path) = path.strip_prefix("//?/UNC/") {
        format!("//{network_path}")
    } else if let Some(drive_path) = path.strip_prefix("//?/")
        && drive_path.as_bytes().get(1) == Some(&b':')
    {
        drive_path.to_owned()
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::windows_git_path;

    #[test]
    fn git_paths_preserve_windows_drive_and_network_roots() {
        for (path, expected) in [
            (r"\\?\UNC\server\share\Work", "//server/share/Work"),
            (r"\\?\C:\Work\Repo", "C:/Work/Repo"),
            (r"\\server\share\Work", "//server/share/Work"),
            (r"C:\Work\Repo", "C:/Work/Repo"),
        ] {
            assert_eq!(windows_git_path(path), expected, "{path}");
        }
    }
}
