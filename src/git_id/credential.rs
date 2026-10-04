use anyhow::{Context, Result};
use gix_error::ResultExt;
use secrecy::ExposeSecret;
use sha2::{Digest, Sha256};
use std::ffi::OsString;

use super::config::{Authentication, load_config};

pub fn run(route_id: &str, operation: &str) -> Result<()> {
    let config_path = super::config::config_path()
        .map_err(|_| anyhow::anyhow!("could not locate the git identity config"))?;
    if !config_path.is_file() {
        anyhow::bail!("could not read the git identity config; run lum git-id sync");
    }
    super::config::ensure_private_config_permissions(&config_path).map_err(|_| {
        anyhow::anyhow!("git identity config permissions are not private; run lum git-id sync")
    })?;
    let args = [OsString::from(operation)];
    let result = gix_credentials::program::main(
        args,
        std::io::stdin().lock(),
        std::io::stdout().lock(),
        gix_credentials::protocol::ContextOptions::default(),
        |action, request| resolve(action, route_id, request).or_erased(),
    );

    result.map_err(|_| {
        anyhow::anyhow!(
            "credential helper request failed; check the git identity config and run lum git-id sync"
        )
    })
}

fn resolve(
    action: gix_credentials::program::main::Action,
    route_id: &str,
    mut request: gix_credentials::protocol::Context,
) -> std::io::Result<Option<gix_credentials::protocol::Context>> {
    let identities = load_config().map_err(|_| credential_unavailable())?;
    let identity = identities
        .iter()
        .find(|identity| self::route_id(&identity.name) == route_id)
        .ok_or_else(credential_unavailable)?;
    let Authentication::HttpBasic {
        scheme,
        username,
        password,
        ..
    } = &identity.authentication
    else {
        return Err(credential_unavailable());
    };

    match action {
        gix_credentials::program::main::Action::Store => Ok(None),
        gix_credentials::program::main::Action::Erase => {
            eprintln!(
                "lum: credentials for identity {} were rejected; update authentication.password in {}",
                identity.name,
                super::config::config_path()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|_| "the git identity config".to_owned())
            );
            Ok(None)
        }
        gix_credentials::program::main::Action::Get => {
            if request.protocol.is_none() || request.host.is_none() {
                request
                    .destructure_url_in_place(false)
                    .map_err(|_| credential_unavailable())?;
            }
            let matches_context = request.protocol.as_deref() == Some(scheme.as_str())
                && request
                    .host
                    .as_deref()
                    .is_some_and(|host| host.eq_ignore_ascii_case(&identity.domain))
                && request
                    .username
                    .as_deref()
                    .is_none_or(|requested| requested == username);
            if !matches_context {
                return Err(credential_unavailable());
            }

            Ok(Some(gix_credentials::protocol::Context {
                username: Some(username.clone()),
                password: Some(password.expose_secret().to_owned()),
                ..Default::default()
            }))
        }
    }
}

fn credential_unavailable() -> std::io::Error {
    std::io::Error::other("credential unavailable")
}

pub fn helper_definition(identity_name: &str) -> Result<String> {
    let executable = std::env::current_exe().context("locating the lum executable")?;
    let executable = super::config::git_path(&executable);
    let command = format!(
        "!{} __git_credential {}",
        shell_quote(&executable),
        route_id(identity_name)
    );
    Ok(git_config_value(&command))
}

pub fn route_id(identity_name: &str) -> String {
    use std::fmt::Write as _;

    let mut route = String::with_capacity(64);
    for byte in Sha256::digest(identity_name.as_bytes()) {
        write!(&mut route, "{byte:02x}").expect("writing to a string cannot fail");
    }
    route
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'\''"#))
}

pub fn git_config_value(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    )
}
