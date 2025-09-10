use crate::{config::PluginSource, git::Selection};

use crate::cli::PluginRepo;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RefKind {
    None,
    Latest,
    Version(String),
    Tag(String),
    Branch(String),
    Commit(String),
}

pub(crate) fn parse_ref_kind(s: &str) -> RefKind {
    if s.eq_ignore_ascii_case("latest") {
        return RefKind::Latest;
    }
    if let Some(rest) = s.strip_prefix("tag:") {
        return RefKind::Tag(rest.to_string());
    }
    if let Some(rest) = s.strip_prefix("branch:") {
        return RefKind::Branch(rest.to_string());
    }
    if let Some(rest) = s.strip_prefix("commit:") {
        return RefKind::Commit(rest.to_string());
    }
    if let Some(rest) = s.strip_prefix("version:") {
        return RefKind::Version(rest.to_string());
    }
    RefKind::Version(s.to_string())
}

pub(crate) fn selection_from_ref_kind(kind: &RefKind) -> Selection {
    match kind {
        RefKind::None => Selection::DefaultHead,
        RefKind::Latest => Selection::Latest,
        RefKind::Version(v) => Selection::Version(v.clone()),
        RefKind::Tag(t) => Selection::Tag(t.clone()),
        RefKind::Branch(b) => Selection::Branch(b.clone()),
        RefKind::Commit(c) => Selection::Commit(c.clone()),
    }
}

pub(crate) fn ref_kind_to_repo_source(repo: &PluginRepo, kind: &RefKind) -> PluginSource {
    match kind {
        RefKind::None => PluginSource::Repo {
            repo: repo.clone(),
            version: None,
            branch: None,
            tag: None,
            commit: None,
        },
        RefKind::Latest => PluginSource::Repo {
            repo: repo.clone(),
            version: Some("latest".to_string()),
            branch: None,
            tag: None,
            commit: None,
        },
        RefKind::Version(v) => PluginSource::Repo {
            repo: repo.clone(),
            version: Some(v.clone()),
            branch: None,
            tag: None,
            commit: None,
        },
        RefKind::Tag(t) => PluginSource::Repo {
            repo: repo.clone(),
            version: None,
            branch: None,
            tag: Some(t.clone()),
            commit: None,
        },
        RefKind::Branch(b) => PluginSource::Repo {
            repo: repo.clone(),
            version: None,
            branch: Some(b.clone()),
            tag: None,
            commit: None,
        },
        RefKind::Commit(c) => PluginSource::Repo {
            repo: repo.clone(),
            version: None,
            branch: None,
            tag: None,
            commit: Some(c.clone()),
        },
    }
}

pub(crate) fn ref_kind_to_url_source(url: &str, kind: &RefKind) -> PluginSource {
    match kind {
        RefKind::None => PluginSource::Url {
            url: url.to_string(),
            version: None,
            branch: None,
            tag: None,
            commit: None,
        },
        RefKind::Latest => PluginSource::Url {
            url: url.to_string(),
            version: Some("latest".to_string()),
            branch: None,
            tag: None,
            commit: None,
        },
        RefKind::Version(v) => PluginSource::Url {
            url: url.to_string(),
            version: Some(v.clone()),
            branch: None,
            tag: None,
            commit: None,
        },
        RefKind::Tag(t) => PluginSource::Url {
            url: url.to_string(),
            version: None,
            branch: None,
            tag: Some(t.clone()),
            commit: None,
        },
        RefKind::Branch(b) => PluginSource::Url {
            url: url.to_string(),
            version: None,
            branch: Some(b.clone()),
            tag: None,
            commit: None,
        },
        RefKind::Commit(c) => PluginSource::Url {
            url: url.to_string(),
            version: None,
            branch: None,
            tag: None,
            commit: Some(c.clone()),
        },
    }
}
