use regex::Regex;
use serde_derive::{Deserialize, Serialize};

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
    pub fn as_str(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
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
        let re = Regex::new(r"^[a-zA-Z0-9-]+/[a-zA-Z0-9_.-]+$").unwrap();
        if re.is_match(s) && !s.ends_with('.') {
            let parts: Vec<&str> = s.split('/').collect();
            Ok(PluginRepo {
                owner: parts[0].to_string(),
                repo: parts[1].to_string(),
            })
        } else {
            Err(format!(
                "Invalid format: {s}. Expected format: <owner>/<repo>"
            ))
        }
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
