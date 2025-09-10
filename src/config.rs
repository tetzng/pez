use serde_derive::{Deserialize, Serialize};
use std::{fs, path};

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
        repo: crate::cli::PluginRepo,
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
    pub(crate) fn get_plugin_repo(&self) -> anyhow::Result<crate::cli::PluginRepo> {
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
                Ok(crate::cli::PluginRepo {
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
                Ok(crate::cli::PluginRepo {
                    owner: "local".to_string(),
                    repo: name,
                })
            }
        }
    }

    /// Convert to a ResolvedInstallTarget for installation flows.
    pub(crate) fn to_resolved(&self) -> anyhow::Result<crate::cli::ResolvedInstallTarget> {
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
                Ok(crate::cli::ResolvedInstallTarget {
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
                Ok(crate::cli::ResolvedInstallTarget {
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
                Ok(crate::cli::ResolvedInstallTarget {
                    plugin_repo,
                    source: expanded,
                    ref_kind: crate::resolver::RefKind::None,
                    is_local: true,
                })
            }
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
