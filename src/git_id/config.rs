use anyhow::{Context, Result};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
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
pub struct GitIdentitiesConfig {
    pub identities: Vec<Identity>,
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

pub fn harden_config_permissions(path: &Path) -> Result<()> {
    let _ = path;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(path, permissions)?;
    }
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
    validate(&config.identities)?;
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
        validate_protocol_scalar(identity, "name", &identity.name)?;
        validate_protocol_scalar(identity, "author_name", &identity.author_name)?;
        validate_protocol_scalar(identity, "email", &identity.email)?;
        validate_domain(identity)?;
        validate_authentication(identity)?;
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
            let normalized = crate::paths::normalize_path(&expanded);
            if !folders.insert(normalized) {
                anyhow::bail!("duplicate managed folder: {}", folder);
            }
        }
    }
    Ok(())
}

fn validate_authentication(identity: &Identity) -> Result<()> {
    let Authentication::HttpBasic {
        scheme,
        username,
        password,
        allow_insecure_http,
    } = &identity.authentication
    else {
        return Ok(());
    };

    if username.trim().is_empty() {
        anyhow::bail!(
            "identity {}: authentication username must not be empty",
            identity.name
        );
    }
    validate_protocol_scalar(identity, "authentication username", username)?;
    let password = password.expose_secret();
    if password.is_empty() {
        anyhow::bail!(
            "identity {}: authentication password must not be empty",
            identity.name
        );
    }
    if password
        .bytes()
        .any(|byte| matches!(byte, b'\0' | b'\n' | b'\r'))
    {
        anyhow::bail!(
            "identity {}: authentication password must not contain NUL, CR, or LF",
            identity.name
        );
    }

    match (scheme, allow_insecure_http) {
        (HttpScheme::Http, false) => anyhow::bail!(
            "identity {}: plain HTTP exposes credentials on the network; set allow_insecure_http to true to acknowledge this",
            identity.name
        ),
        (HttpScheme::Https, true) => anyhow::bail!(
            "identity {}: allow_insecure_http is only valid with the http scheme",
            identity.name
        ),
        _ => Ok(()),
    }
}

fn validate_domain(identity: &Identity) -> Result<()> {
    let candidate = format!("https://{}/", identity.domain);
    let url = url::Url::parse(&candidate).with_context(|| {
        format!(
            "identity {}: domain must be a host with an optional port",
            identity.name
        )
    })?;
    if url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        anyhow::bail!(
            "identity {}: domain must be a host with an optional port, without a scheme or path",
            identity.name
        );
    }
    Ok(())
}

fn validate_protocol_scalar(identity: &Identity, field: &str, value: &str) -> Result<()> {
    if value
        .bytes()
        .any(|byte| matches!(byte, b'\0' | b'\n' | b'\r'))
    {
        anyhow::bail!(
            "identity {}: {} must not contain NUL, CR, or LF",
            identity.name,
            field
        );
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
    Ok(identity_private_key_path(identity)?.with_extension("pub"))
}

pub fn identity_git_config_path(identity: &Identity) -> Result<PathBuf> {
    Ok(crate::paths::home_dir()?.join(format!(".gitconfig-lum-git-id-{}", identity.name)))
}

pub fn allowed_signers_path() -> Result<PathBuf> {
    crate::paths::home_path(".ssh/allowed_signers")
}

pub fn git_path(path: &std::path::Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
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
    use std::path::Path;

    use super::git_path;

    #[test]
    fn git_paths_preserve_windows_drive_and_network_roots() {
        for (path, expected) in [
            (r"\\?\UNC\server\share\Work", "//server/share/Work"),
            (r"\\?\C:\Work\Repo", "C:/Work/Repo"),
            (r"\\server\share\Work", "//server/share/Work"),
            (r"C:\Work\Repo", "C:/Work/Repo"),
            ("/home/jane/Work", "/home/jane/Work"),
        ] {
            assert_eq!(git_path(Path::new(path)), expected, "{path}");
        }
    }
}
