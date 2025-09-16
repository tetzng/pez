use serde_derive::{Deserialize, Serialize};
use std::{fs, path};

use crate::models::{PluginRepo, ResolvedInstallTarget};
use crate::resolver::{ref_kind_to_repo_source, ref_kind_to_url_source};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct Config {
    pub(crate) plugins: Option<Vec<PluginSpec>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct PluginSpec {
    pub(crate) name: Option<String>,
    #[serde(flatten)]
    pub(crate) source: PluginSource,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub(crate) enum PluginSource {
    // GitHub shorthand: { repo = "owner/repo", [version|branch|tag|commit] = "..." }
    Repo {
        repo: crate::models::PluginRepo,
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        branch: Option<String>,
        #[serde(default)]
        tag: Option<String>,
        #[serde(default)]
        commit: Option<String>,
    },
    // Generic Git: { url = "https://host/owner/repo", [version|branch|tag|commit] = "..." }
    Url {
        url: String,
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        branch: Option<String>,
        #[serde(default)]
        tag: Option<String>,
        #[serde(default)]
        commit: Option<String>,
    },
    // Local path: { path = "~/plugins/foo" }
    Path {
        path: String,
    },
}

pub(crate) fn init() -> Config {
    Config { plugins: None }
}

pub(crate) fn load(path: &path::PathBuf) -> anyhow::Result<Config> {
    let content = fs::read_to_string(path)?;
    let config = toml::from_str(&content)?;

    Ok(config)
}

impl Config {
    pub(crate) fn save(&self, path: &path::PathBuf) -> anyhow::Result<()> {
        let contents = toml::to_string(self)?;
        fs::write(path, contents)?;

        Ok(())
    }

    /// Ensure that the config contains a plugin entry derived from the provided resolved target.
    /// Returns true when a new entry is inserted.
    pub(crate) fn ensure_plugin_from_resolved(&mut self, resolved: &ResolvedInstallTarget) -> bool {
        let plugin_specs = self.plugins.get_or_insert_with(Vec::new);
        if plugin_specs.iter().any(|spec| {
            spec.get_plugin_repo()
                .is_ok_and(|repo| repo == resolved.plugin_repo)
        }) {
            return false;
        }

        plugin_specs.push(PluginSpec::from_resolved(resolved));
        true
    }

    /// Ensure that the config contains a default entry for the provided repo.
    /// Returns true when a new entry is inserted.
    pub(crate) fn ensure_plugin_for_repo(&mut self, plugin_repo: &PluginRepo) -> bool {
        let resolved = ResolvedInstallTarget {
            plugin_repo: plugin_repo.clone(),
            source: format!("https://github.com/{}", plugin_repo.as_str()),
            ref_kind: crate::resolver::RefKind::None,
            is_local: false,
        };
        self.ensure_plugin_from_resolved(&resolved)
    }
}

impl PluginSpec {
    pub(crate) fn get_name(&self) -> anyhow::Result<String> {
        if let Some(name) = &self.name {
            return Ok(name.clone());
        }
        let repo = self.get_plugin_repo()?;
        Ok(repo.repo)
    }

    /// Derive a PluginRepo (owner/repo) for use as a stable identifier and data dir name.
    /// - Github: uses provided owner/repo
    /// - Git URL: attempts to parse last two path segments as owner/repo
    /// - Path: owner = "local", repo = basename of path
    pub(crate) fn get_plugin_repo(&self) -> anyhow::Result<crate::models::PluginRepo> {
        match &self.source {
            PluginSource::Repo { repo, .. } => Ok(repo.clone()),
            PluginSource::Url { url, .. } => {
                // Parse last two segments from URL path
                let repo_name = url
                    .trim_end_matches('/')
                    .trim_end_matches(".git")
                    .rsplit('/')
                    .next()
                    .unwrap_or("repo")
                    .to_string();
                let owner = url
                    .trim_end_matches('/')
                    .rsplit('/')
                    .nth(1)
                    .unwrap_or("owner")
                    .to_string();
                Ok(crate::models::PluginRepo {
                    owner,
                    repo: repo_name,
                })
            }
            PluginSource::Path { path } => {
                let expanded = expand_tilde(path)?;
                let name = std::path::Path::new(&expanded)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| anyhow::anyhow!("Invalid local path: {expanded}"))?
                    .to_string();
                Ok(crate::models::PluginRepo {
                    owner: "local".to_string(),
                    repo: name,
                })
            }
        }
    }

    /// Convert to a ResolvedInstallTarget for installation flows.
    pub(crate) fn to_resolved(&self) -> anyhow::Result<crate::models::ResolvedInstallTarget> {
        let plugin_repo = self.get_plugin_repo()?;
        match &self.source {
            PluginSource::Repo {
                repo: _,
                version,
                branch,
                tag,
                commit,
            } => {
                let src = format!("https://github.com/{}", plugin_repo.as_str());
                let refspec = pick_single_ref(version, branch, tag, commit)?;
                Ok(crate::models::ResolvedInstallTarget {
                    plugin_repo,
                    source: src,
                    ref_kind: crate::resolver::RefKind::from(refspec),
                    is_local: false,
                })
            }
            PluginSource::Url {
                url,
                version,
                branch,
                tag,
                commit,
            } => {
                let mut normalized = url.clone();
                if !normalized.contains("://") {
                    normalized = format!("https://{normalized}");
                }
                let refspec = pick_single_ref(version, branch, tag, commit)?;
                Ok(crate::models::ResolvedInstallTarget {
                    plugin_repo,
                    source: normalized,
                    ref_kind: crate::resolver::RefKind::from(refspec),
                    is_local: false,
                })
            }
            PluginSource::Path { path } => {
                let expanded = expand_tilde(path)?;
                if !expanded.starts_with('/') {
                    anyhow::bail!(
                        "path must be absolute or start with ~/ (after expansion must be absolute)"
                    );
                }
                Ok(crate::models::ResolvedInstallTarget {
                    plugin_repo,
                    source: expanded,
                    ref_kind: crate::resolver::RefKind::None,
                    is_local: true,
                })
            }
        }
    }

    pub(crate) fn from_resolved(resolved: &ResolvedInstallTarget) -> Self {
        let source = if resolved.is_local {
            PluginSource::Path {
                path: resolved.source.clone(),
            }
        } else {
            let default_source = format!("https://github.com/{}", resolved.plugin_repo.as_str());
            if resolved.source == default_source {
                ref_kind_to_repo_source(&resolved.plugin_repo, &resolved.ref_kind)
            } else {
                ref_kind_to_url_source(&resolved.source, &resolved.ref_kind)
            }
        };

        PluginSpec { name: None, source }
    }
}

#[cfg(test)]
mod internal_tests {
    use super::*;

    #[test]
    fn repo_to_resolved_latest() {
        let s = PluginSource::Repo {
            repo: crate::models::PluginRepo {
                owner: "o".into(),
                repo: "r".into(),
            },
            version: Some("latest".into()),
            branch: None,
            tag: None,
            commit: None,
        };
        let spec = PluginSpec {
            name: None,
            source: s,
        };
        let r = spec.to_resolved().unwrap();
        assert_eq!(r.source, "https://github.com/o/r");
        matches!(r.ref_kind, crate::resolver::RefKind::Latest);
    }

    #[test]
    fn url_without_scheme_normalizes() {
        let s = PluginSource::Url {
            url: "gitlab.com/o/r".into(),
            version: Some("v3".into()),
            branch: None,
            tag: None,
            commit: None,
        };
        let spec = PluginSpec {
            name: None,
            source: s,
        };
        let r = spec.to_resolved().unwrap();
        assert_eq!(r.source, "https://gitlab.com/o/r");
        matches!(r.ref_kind, crate::resolver::RefKind::Version(_));
    }

    #[test]
    fn path_requires_absolute_or_tilde() {
        let s = PluginSource::Path {
            path: "relative/path".into(),
        };
        let spec = PluginSpec {
            name: None,
            source: s,
        };
        let err = spec.to_resolved().unwrap_err();
        assert!(err.to_string().contains("absolute"));
    }

    #[test]
    fn one_of_rule_enforced() {
        let s = PluginSource::Repo {
            repo: crate::models::PluginRepo {
                owner: "o".into(),
                repo: "r".into(),
            },
            version: Some("v1".into()),
            branch: None,
            tag: Some("v1.0.0".into()),
            commit: None,
        };
        let spec = PluginSpec {
            name: None,
            source: s,
        };
        let err = spec.to_resolved().unwrap_err();
        assert!(err.to_string().contains("Multiple version selectors"));
    }

    #[test]
    fn from_resolved_builds_repo_spec() {
        let resolved = ResolvedInstallTarget {
            plugin_repo: PluginRepo {
                owner: "o".into(),
                repo: "r".into(),
            },
            source: "https://github.com/o/r".into(),
            ref_kind: crate::resolver::RefKind::Branch("dev".into()),
            is_local: false,
        };

        let spec = PluginSpec::from_resolved(&resolved);

        match spec.source {
            PluginSource::Repo {
                repo,
                branch,
                version,
                tag,
                commit,
            } => {
                assert_eq!(repo.owner, "o");
                assert_eq!(repo.repo, "r");
                assert_eq!(branch.as_deref(), Some("dev"));
                assert!(version.is_none());
                assert!(tag.is_none());
                assert!(commit.is_none());
            }
            other => panic!("expected repo source, got {other:?}"),
        }
    }

    #[test]
    fn from_resolved_builds_url_spec() {
        let resolved = ResolvedInstallTarget {
            plugin_repo: PluginRepo {
                owner: "o".into(),
                repo: "r".into(),
            },
            source: "https://gitlab.com/o/r".into(),
            ref_kind: crate::resolver::RefKind::Tag("v1.0.0".into()),
            is_local: false,
        };

        let spec = PluginSpec::from_resolved(&resolved);

        match spec.source {
            PluginSource::Url {
                url,
                tag,
                version,
                branch,
                commit,
            } => {
                assert_eq!(url, "https://gitlab.com/o/r");
                assert_eq!(tag.as_deref(), Some("v1.0.0"));
                assert!(version.is_none());
                assert!(branch.is_none());
                assert!(commit.is_none());
            }
            other => panic!("expected url source, got {other:?}"),
        }
    }

    #[test]
    fn from_resolved_builds_path_spec() {
        let resolved = ResolvedInstallTarget {
            plugin_repo: PluginRepo {
                owner: "local".into(),
                repo: "tool".into(),
            },
            source: "/tmp/tool".into(),
            ref_kind: crate::resolver::RefKind::None,
            is_local: true,
        };

        let spec = PluginSpec::from_resolved(&resolved);

        match spec.source {
            PluginSource::Path { path } => assert_eq!(path, "/tmp/tool"),
            other => panic!("expected path source, got {other:?}"),
        }
    }

    #[test]
    fn ensure_plugin_from_resolved_inserts_once() {
        let mut config = Config { plugins: None };
        let resolved = ResolvedInstallTarget {
            plugin_repo: PluginRepo {
                owner: "o".into(),
                repo: "r".into(),
            },
            source: "https://github.com/o/r".into(),
            ref_kind: crate::resolver::RefKind::None,
            is_local: false,
        };

        assert!(config.ensure_plugin_from_resolved(&resolved));
        assert_eq!(config.plugins.as_ref().unwrap().len(), 1);
        assert!(!config.ensure_plugin_from_resolved(&resolved));
        assert_eq!(config.plugins.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn ensure_plugin_for_repo_inserts_default_spec() {
        let mut config = Config { plugins: None };
        let repo = PluginRepo {
            owner: "o".into(),
            repo: "r".into(),
        };

        assert!(config.ensure_plugin_for_repo(&repo));
        let specs = config.plugins.unwrap();
        assert_eq!(specs.len(), 1);
        match specs[0].source.clone() {
            PluginSource::Repo {
                repo: spec_repo, ..
            } => assert_eq!(spec_repo, repo),
            other => panic!("expected repo source, got {other:?}"),
        }
    }
}
fn expand_tilde(p: &str) -> anyhow::Result<String> {
    if let Some(stripped) = p.strip_prefix("~/") {
        let home = std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("HOME not set"))?;
        Ok(std::path::Path::new(&home)
            .join(stripped)
            .to_string_lossy()
            .to_string())
    } else if p == "~" {
        let home = std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("HOME not set"))?;
        Ok(std::path::PathBuf::from(home).to_string_lossy().to_string())
    } else {
        Ok(p.to_string())
    }
}

fn pick_single_ref(
    version: &Option<String>,
    branch: &Option<String>,
    tag: &Option<String>,
    commit: &Option<String>,
) -> anyhow::Result<Option<String>> {
    let mut vals = vec![];
    if let Some(v) = version {
        vals.push(("version", v.clone()));
    }
    if let Some(v) = branch {
        vals.push(("branch", v.clone()));
    }
    if let Some(v) = tag {
        vals.push(("tag", v.clone()));
    }
    if let Some(v) = commit {
        vals.push(("commit", v.clone()));
    }
    if vals.len() > 1 {
        let kinds = vals.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(", ");
        anyhow::bail!(format!(
            "Multiple version selectors set: {kinds}. Please specify only one of version, branch, tag, or commit."
        ));
    }
    Ok(vals.into_iter().next().map(|(_, v)| v))
}

impl From<Option<String>> for crate::resolver::RefKind {
    fn from(val: Option<String>) -> Self {
        match val {
            None => crate::resolver::RefKind::None,
            Some(x) => {
                if x.eq_ignore_ascii_case("latest") {
                    crate::resolver::RefKind::Latest
                } else {
                    crate::resolver::RefKind::Version(x)
                }
            }
        }
    }
}

use crate::resolver::RefKind;

#[allow(dead_code)]
fn ref_to_kind(val: Option<String>) -> RefKind {
    match val {
        None => RefKind::None,
        Some(x) => {
            if x.eq_ignore_ascii_case("latest") {
                return RefKind::Latest;
            }
            RefKind::Version(x)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_to_resolved_latest() {
        let s = PluginSource::Repo {
            repo: crate::models::PluginRepo {
                owner: "o".into(),
                repo: "r".into(),
            },
            version: Some("latest".into()),
            branch: None,
            tag: None,
            commit: None,
        };
        let spec = PluginSpec {
            name: None,
            source: s,
        };
        let r = spec.to_resolved().unwrap();
        assert_eq!(r.source, "https://github.com/o/r");
        matches!(r.ref_kind, crate::resolver::RefKind::Latest);
    }

    #[test]
    fn url_without_scheme_normalizes() {
        let s = PluginSource::Url {
            url: "gitlab.com/o/r".into(),
            version: Some("v3".into()),
            branch: None,
            tag: None,
            commit: None,
        };
        let spec = PluginSpec {
            name: None,
            source: s,
        };
        let r = spec.to_resolved().unwrap();
        assert_eq!(r.source, "https://gitlab.com/o/r");
        matches!(r.ref_kind, crate::resolver::RefKind::Version(_));
    }

    #[test]
    fn path_requires_absolute_or_tilde() {
        let s = PluginSource::Path {
            path: "relative/path".into(),
        };
        let spec = PluginSpec {
            name: None,
            source: s,
        };
        let err = spec.to_resolved().unwrap_err();
        assert!(err.to_string().contains("absolute"));
    }

    #[test]
    fn one_of_rule_enforced() {
        let s = PluginSource::Repo {
            repo: crate::models::PluginRepo {
                owner: "o".into(),
                repo: "r".into(),
            },
            version: Some("v1".into()),
            branch: None,
            tag: Some("v1.0.0".into()),
            commit: None,
        };
        let spec = PluginSpec {
            name: None,
            source: s,
        };
        let err = spec.to_resolved().unwrap_err();
        assert!(err.to_string().contains("Multiple version selectors"));
    }
}
