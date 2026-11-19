//! Completers for the names a shell cannot know: catalog entries and what
//! lum's config files hold. Each returns every name and the shell filters
//! them by what was typed; a config that cannot be read completes nothing.

use std::collections::BTreeSet;

use usage_rs::complete::{Candidate, CompleteCtx};

type Candidates = Vec<Candidate<'static>>;

pub(super) fn stations<P>(_: &P, _: &CompleteCtx<'_>) -> Candidates {
    crate::radio::stations::all()
        .iter()
        .map(|station| Candidate::described(station.code, station.name))
        .collect()
}

pub(super) fn tools<P>(_: &P, _: &CompleteCtx<'_>) -> Candidates {
    crate::tools::catalog::CATALOG
        .iter()
        .map(|tool| Candidate::described(tool.name, tool.description))
        .collect()
}

pub(super) fn apps<P>(_: &P, _: &CompleteCtx<'_>) -> Candidates {
    crate::apps::catalog::CATALOG
        .iter()
        .map(|app| Candidate::described(app.name, app.description))
        .collect()
}

pub(super) fn fonts<P>(_: &P, _: &CompleteCtx<'_>) -> Candidates {
    crate::fonts::catalog::CATALOG
        .iter()
        .map(|font| Candidate::described(font.name, font.description))
        .collect()
}

pub(super) fn env_aliases<P>(_: &P, _: &CompleteCtx<'_>) -> Candidates {
    crate::env::catalog::ALIASES
        .iter()
        .map(|(alias, variable)| Candidate::described(*alias, *variable))
        .collect()
}

pub(super) fn identities<P>(_: &P, _: &CompleteCtx<'_>) -> Candidates {
    crate::git_id::config::load_config()
        .unwrap_or_default()
        .into_iter()
        .map(|identity| Candidate::described(identity.name, identity.email))
        .collect()
}

pub(super) fn mirror_tags<P>(_: &P, _: &CompleteCtx<'_>) -> Candidates {
    use crate::repos::mirror::config;
    let tags: BTreeSet<String> = config::config_path()
        .and_then(|path| config::load(&path))
        .unwrap_or_default()
        .into_iter()
        .flat_map(|repo| repo.tags)
        .collect();
    tags.into_iter().map(Candidate::new).collect()
}

pub(super) fn wired_interfaces<P>(_: &P, _: &CompleteCtx<'_>) -> Candidates {
    crate::net::wired_interfaces()
        .into_iter()
        .map(Candidate::new)
        .collect()
}
