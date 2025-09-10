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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git;

    #[test]
    fn parses_ref_kinds() {
        assert!(matches!(parse_ref_kind("latest"), RefKind::Latest));
        assert!(matches!(parse_ref_kind("tag:v1.0.0"), RefKind::Tag(t) if t=="v1.0.0"));
        assert!(matches!(parse_ref_kind("branch:dev"), RefKind::Branch(b) if b=="dev"));
        assert!(matches!(parse_ref_kind("commit:abc1234"), RefKind::Commit(c) if c=="abc1234"));
        assert!(matches!(parse_ref_kind("version:v3"), RefKind::Version(v) if v=="v3"));
        assert!(matches!(parse_ref_kind("v3"), RefKind::Version(v) if v=="v3"));
    }

    #[test]
    fn maps_to_selection() {
        let sel = selection_from_ref_kind(&RefKind::Latest);
        match sel {
            git::Selection::Latest => {}
            _ => panic!(),
        }
        let sel = selection_from_ref_kind(&RefKind::Branch("main".into()));
        match sel {
            git::Selection::Branch(b) => assert_eq!(b, "main"),
            _ => panic!(),
        }
        let sel = selection_from_ref_kind(&RefKind::Tag("v1".into()));
        match sel {
            git::Selection::Tag(t) => assert_eq!(t, "v1"),
            _ => panic!(),
        }
        let sel = selection_from_ref_kind(&RefKind::Commit("abc".into()));
        match sel {
            git::Selection::Commit(c) => assert_eq!(c, "abc"),
            _ => panic!(),
        }
        let sel = selection_from_ref_kind(&RefKind::Version("v3".into()));
        match sel {
            git::Selection::Version(v) => assert_eq!(v, "v3"),
            _ => panic!(),
        }
    }
}
