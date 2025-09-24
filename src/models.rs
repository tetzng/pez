use regex::Regex;
use serde_derive::{Deserialize, Serialize};
use url::Url;

// Generic destination directory kinds for fish assets

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub(crate) enum TargetDir {
    #[serde(rename = "functions")]
    Functions,
    #[serde(rename = "completions")]
    Completions,
    #[serde(rename = "conf.d")]
    ConfD,
    #[serde(rename = "themes")]
    Themes,
}

impl TargetDir {
    pub(crate) fn as_str(&self) -> &str {
        match self {
            TargetDir::Functions => "functions",
            TargetDir::Completions => "completions",
            TargetDir::ConfD => "conf.d",
            TargetDir::Themes => "themes",
        }
    }
    pub(crate) fn all() -> Vec<Self> {
        vec![
            TargetDir::Functions,
            TargetDir::Completions,
            TargetDir::ConfD,
            TargetDir::Themes,
        ]
    }
}

impl std::str::FromStr for TargetDir {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "functions" => Ok(TargetDir::Functions),
            "completions" => Ok(TargetDir::Completions),
            "conf.d" => Ok(TargetDir::ConfD),
            "themes" => Ok(TargetDir::Themes),
            _ => Err(format!("Invalid target dir: {s}")),
        }
    }
}

// Core typed identifiers and inputs used across CLI and core logic

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(try_from = "String", into = "String")]
pub(crate) struct PluginRepo {
    pub host: Option<String>,
    pub owner: String,
    pub repo: String,
}

impl TryFrom<String> for PluginRepo {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<PluginRepo> for String {
    fn from(plugin_repo: PluginRepo) -> Self {
        plugin_repo.as_str()
    }
}

impl PluginRepo {
    pub fn new(host: Option<String>, owner: String, repo: String) -> Result<Self, String> {
        validate_repo_segment(&owner)
            .map_err(|e| format!("Invalid owner segment '{owner}': {e}"))?;
        validate_repo_segment(&repo).map_err(|e| format!("Invalid repo segment '{repo}': {e}"))?;
        if let Some(ref host_str) = host {
            validate_host_segment(host_str)
                .map_err(|e| format!("Invalid host segment '{host_str}': {e}"))?;
        }
        Ok(Self { host, owner, repo })
    }

    pub fn as_str(&self) -> String {
        match &self.host {
            Some(host) => format!("{}/{}/{}", host, self.owner, self.repo),
            None => format!("{}/{}", self.owner, self.repo),
        }
    }

    pub fn owner_repo_path(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    pub fn default_remote_source(&self) -> String {
        match &self.host {
            Some(host) => format!("https://{host}/{}", self.owner_repo_path()),
            None => format!("https://github.com/{}", self.owner_repo_path()),
        }
    }

    pub fn from_remote_url(raw: &str) -> Option<Self> {
        parse_standard_url(raw)
            .or_else(|| parse_scp_like(raw))
            .and_then(|(host, owner, repo)| PluginRepo::new(host, owner, repo).ok())
    }
}

impl std::fmt::Display for PluginRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for PluginRepo {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            return Err("Plugin repo cannot be empty".to_string());
        }

        let parts: Vec<&str> = s.split('/').collect();
        match parts.as_slice() {
            [owner, repo] => PluginRepo::new(None, (*owner).to_string(), (*repo).to_string()),
            [host, owner, repo] => PluginRepo::new(
                Some((*host).to_string()),
                (*owner).to_string(),
                (*repo).to_string(),
            ),
            _ => Err(format!(
                "Invalid format: {s}. Expected <owner>/<repo> or <host>/<owner>/<repo>"
            )),
        }
    }
}

fn validate_repo_segment(segment: &str) -> Result<(), &'static str> {
    let re = Regex::new(r"^[a-zA-Z0-9_.-]+$").unwrap();
    if re.is_match(segment) && !segment.ends_with('.') {
        Ok(())
    } else {
        Err("must contain letters, digits, underscore, dot, or dash")
    }
}

fn validate_host_segment(segment: &str) -> Result<(), &'static str> {
    let re = Regex::new(r"^[a-zA-Z0-9.-]+$").unwrap();
    if re.is_match(segment) && !segment.starts_with('.') && !segment.ends_with('.') {
        Ok(())
    } else {
        Err("must contain letters, digits, dot, or dash without leading/trailing dots")
    }
}

fn parse_standard_url(raw: &str) -> Option<(Option<String>, String, String)> {
    let parsed = Url::parse(raw).ok()?;
    if parsed.scheme() == "file" {
        return None;
    }
    let host_str = parsed.host_str().map(|s| s.to_string());
    let host = match host_str {
        Some(ref h) if h.eq_ignore_ascii_case("github.com") => None,
        other => other,
    };
    let mut segments: Vec<String> = parsed
        .path()
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches(".git").to_string())
        .collect();
    if segments.len() < 2 {
        return None;
    }
    let repo = segments.pop()?;
    let owner = segments.pop()?;
    Some((host, owner, repo))
}

fn parse_scp_like(raw: &str) -> Option<(Option<String>, String, String)> {
    if raw.contains("://") {
        return None;
    }
    let (host_part, path_part) = raw.split_once(':')?;
    let host_str = host_part
        .strip_prefix("git@")
        .unwrap_or(host_part)
        .to_string();
    let host = if host_str.eq_ignore_ascii_case("github.com") {
        None
    } else {
        Some(host_str)
    };
    let path = path_part.trim_start_matches('/');
    let mut segments: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches(".git").to_string())
        .collect();
    if segments.len() < 2 {
        return None;
    }
    let repo = segments.pop()?;
    let owner = segments.pop()?;
    Some((host, owner, repo))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_owner_repo_pair() {
        let repo: PluginRepo = "owner/repo".parse().unwrap();
        assert_eq!(repo.host.as_deref(), None);
        assert_eq!(repo.owner, "owner");
        assert_eq!(repo.repo, "repo");
        assert_eq!(repo.as_str(), "owner/repo");
        assert_eq!(
            repo.default_remote_source(),
            "https://github.com/owner/repo"
        );
    }

    #[test]
    fn parses_host_prefixed_pair() {
        let repo: PluginRepo = "gitlab.com/owner/repo".parse().unwrap();
        assert_eq!(repo.host.as_deref(), Some("gitlab.com"));
        assert_eq!(repo.as_str(), "gitlab.com/owner/repo");
        assert_eq!(
            repo.default_remote_source(),
            "https://gitlab.com/owner/repo"
        );
    }

    #[test]
    fn parses_https_remote() {
        let repo = PluginRepo::from_remote_url("https://gitlab.com/owner/repo.git").unwrap();
        assert_eq!(repo.host.as_deref(), Some("gitlab.com"));
        assert_eq!(repo.owner, "owner");
        assert_eq!(repo.repo, "repo");
    }

    #[test]
    fn normalizes_github_remotes_to_default_host() {
        let repo = PluginRepo::from_remote_url("https://github.com/owner/repo").unwrap();
        assert_eq!(repo.host.as_deref(), None);
        assert_eq!(repo.as_str(), "owner/repo");
    }

    #[test]
    fn parses_scp_like_remote() {
        let repo = PluginRepo::from_remote_url("git@bitbucket.org:team/pkg.git").unwrap();
        assert_eq!(repo.host.as_deref(), Some("bitbucket.org"));
        assert_eq!(repo.owner, "team");
        assert_eq!(repo.repo, "pkg");
    }
}

/// A user-supplied install target that can be a repo, URL, or local path.
/// Supported examples:
/// - `owner/repo`
/// - `owner/repo@v3`
/// - `gitlab.com/owner/repo`
/// - `gitlab.com/owner/repo@branch`
/// - <https://example.com/owner/repo>
/// - `~/path/to/repo` or `./relative/path`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(try_from = "String", into = "String")]
pub(crate) struct InstallTarget {
    pub(crate) raw: String,
}

impl TryFrom<String> for InstallTarget {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(InstallTarget { raw: value })
    }
}

impl From<InstallTarget> for String {
    fn from(val: InstallTarget) -> Self {
        val.raw
    }
}

impl std::str::FromStr for InstallTarget {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(InstallTarget { raw: s.to_string() })
    }
}

/// Result of parsing an `InstallTarget` into concrete fields used by commands.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedInstallTarget {
    pub plugin_repo: PluginRepo,
    /// Repository base source (URL or local path, without @ref).
    pub source: String,
    /// Optional ref selection.
    pub ref_kind: crate::resolver::RefKind,
    /// Whether the source is a local filesystem path.
    pub is_local: bool,
}
