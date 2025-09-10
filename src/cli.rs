use clap::{Args, Parser, Subcommand};
use regex::Regex;
use serde_derive::{Deserialize, Serialize};
use std::fmt;

#[derive(Parser, Debug)]
#[command(name = "pez", version, about, long_about = None)]
pub(crate) struct Cli {
    /// Increase output verbosity (-v for info, -vv for debug)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub(crate) verbose: u8,

    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    /// Initialize pez
    Init,

    /// Install fish plugin(s)
    Install(InstallArgs),

    /// Uninstall fish plugin(s)
    Uninstall(UninstallArgs),

    /// Upgrade installed fish plugin(s)
    Upgrade(UpgradeArgs),

    /// List installed fish plugins
    List(ListArgs),

    /// Prune uninstalled plugins
    Prune(PruneArgs),

    /// Generate shell completion scripts
    Completions {
        #[arg(value_enum)]
        shell: ShellType,
    },

    /// Diagnose common setup issues
    Doctor(DoctorArgs),

    /// Migrate from fisher (reads fish_plugins)
    Migrate(MigrateArgs),
}

#[derive(Args, Debug)]
pub(crate) struct InstallArgs {
    /// Plugin sources: `owner/repo[@ref]`, `host/owner/repo[@ref]`, full URL, or local path
    pub(crate) plugins: Option<Vec<InstallTarget>>,

    /// Force install even if the plugin is already installed
    #[arg(short, long)]
    pub(crate) force: bool,

    /// Prune uninstalled plugins
    #[arg(short, long, conflicts_with = "plugins")]
    pub(crate) prune: bool,
}

#[derive(Args, Debug)]
pub(crate) struct UninstallArgs {
    /// GitHub repo in the format `owner/repo`
    #[arg(required = true)]
    pub(crate) plugins: Vec<PluginRepo>,

    /// Force uninstall even if the plugin data directory does not exist
    #[arg(short, long)]
    pub(crate) force: bool,
}

#[derive(Args, Debug)]
pub(crate) struct UpgradeArgs {
    /// GitHub repo in the format `owner/repo`
    pub(crate) plugins: Option<Vec<PluginRepo>>,
}

#[derive(Args, Debug)]
pub(crate) struct ListArgs {
    /// List format
    #[arg(long, value_enum)]
    pub(crate) format: Option<ListFormat>,

    /// Show only outdated plugins
    #[arg(long)]
    pub(crate) outdated: bool,
}

#[derive(Args, Debug)]
pub(crate) struct PruneArgs {
    /// Force prune even if the plugin data directory does not exist
    #[arg(short, long)]
    pub(crate) force: bool,

    /// Dry run without actually removing any files
    #[arg(long)]
    pub(crate) dry_run: bool,

    /// Confirm all prompts
    #[arg(short, long)]
    pub(crate) yes: bool,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub(crate) enum ListFormat {
    Plain,
    Table,
    Json,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub(crate) enum ShellType {
    Fish,
}

#[derive(Args, Debug)]
pub(crate) struct DoctorArgs {
    /// Output format
    #[arg(long, value_enum)]
    pub(crate) format: Option<DoctorFormat>,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub(crate) enum DoctorFormat {
    Json,
}

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

impl fmt::Display for PluginRepo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
    raw: String,
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

impl InstallTarget {
    /// Create an InstallTarget from a raw string.
    pub fn from_raw<S: Into<String>>(s: S) -> Self {
        InstallTarget { raw: s.into() }
    }
    /// Parse the raw string into a `ResolvedInstallTarget`.
    /// Rules:
    /// - `owner/repo[@ref]` => github.com
    /// - `host/owner/repo[@ref]` (no scheme) => <https://host/owner/repo>
    /// - URLs with scheme left as-is (no @ref parsing to avoid ssh user@ conflicts)
    /// - Paths beginning with '/', './', '../', or '~' are treated as local
    pub fn resolve(&self) -> anyhow::Result<ResolvedInstallTarget> {
        use anyhow::Context;
        let raw = self.raw.trim();

        // Local path detection
        let looks_like_path = raw.starts_with('/')
            || raw.starts_with("./")
            || raw.starts_with("../")
            || raw.starts_with('~');

        // URL/scheme detection
        let has_scheme =
            raw.contains("://") || raw.starts_with("git@") || raw.starts_with("ssh://");

        // Helper to expand ~
        let expand_tilde = |p: &str| -> anyhow::Result<String> {
            if let Some(stripped) = p.strip_prefix("~/") {
                let home =
                    std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("HOME not set"))?;
                Ok(std::path::Path::new(&home)
                    .join(stripped)
                    .to_string_lossy()
                    .to_string())
            } else if p == "~" {
                let home =
                    std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("HOME not set"))?;
                Ok(std::path::PathBuf::from(home).to_string_lossy().to_string())
            } else {
                Ok(p.to_string())
            }
        };

        if looks_like_path {
            let path_str = expand_tilde(raw)?;
            let plugin_name = std::path::Path::new(&path_str)
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow::anyhow!("Invalid local path: {path_str}"))?
                .to_string();
            let plugin_repo = PluginRepo {
                owner: "local".to_string(),
                repo: plugin_name,
            };
            return Ok(ResolvedInstallTarget {
                plugin_repo,
                source: path_str,
                ref_kind: crate::resolver::RefKind::None,
                is_local: true,
            });
        }

        // Full URL (leave as-is; no @ref parsing to avoid ssh user@host conflict)
        if has_scheme {
            let url = raw.to_string();
            // Try to infer owner/repo from path for naming, else fallback to last segment
            let repo_name = url
                .rsplit('/')
                .next()
                .map(|s| s.trim_end_matches(".git"))
                .unwrap_or("repo")
                .to_string();
            let plugin_repo = PluginRepo {
                owner: "url".to_string(),
                repo: repo_name,
            };
            return Ok(ResolvedInstallTarget {
                plugin_repo,
                source: url,
                ref_kind: crate::resolver::RefKind::None,
                is_local: false,
            });
        }

        // host/owner/repo[@ref] or owner/repo[@ref]
        let (base, ref_kind) = match raw.split_once('@') {
            Some((lhs, rhs)) => (lhs.to_string(), crate::resolver::parse_ref_kind(rhs)),
            None => (raw.to_string(), crate::resolver::RefKind::None),
        };

        let parts: Vec<&str> = base.split('/').collect();
        if parts.len() == 2 {
            // owner/repo -> default host github.com
            let owner = parts[0].to_string();
            let repo = parts[1].to_string();
            let plugin_repo = PluginRepo { owner, repo };
            let source = format!("https://github.com/{}", plugin_repo.as_str());
            return Ok(ResolvedInstallTarget {
                plugin_repo,
                source,
                ref_kind,
                is_local: false,
            });
        } else if parts.len() == 3 {
            // host/owner/repo -> https host
            let host = parts[0];
            let owner = parts[1].to_string();
            let repo = parts[2].to_string();
            let plugin_repo = PluginRepo { owner, repo };
            let source = format!("https://{host}/{}", plugin_repo.as_str());
            return Ok(ResolvedInstallTarget {
                plugin_repo,
                source,
                ref_kind,
                is_local: false,
            });
        }

        Err(anyhow::anyhow!(format!(
            "Invalid plugin source: {raw}. Expected <owner>/<repo>[@ref], <host>/<owner>/<repo>[@ref], URL, or local path"
        )))
            .context("Failed to parse install target")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_owner_repo_and_variants() {
        // owner/repo (github)
        let t: InstallTarget = "o/r".parse().unwrap();
        let r = t.resolve().unwrap();
        assert_eq!(r.plugin_repo.as_str(), "o/r");
        assert_eq!(r.source, "https://github.com/o/r");
        assert!(!r.is_local);

        // owner/repo@v3 -> Version
        let t: InstallTarget = "o/r@v3".parse().unwrap();
        let r = t.resolve().unwrap();
        matches!(r.ref_kind, crate::resolver::RefKind::Version(_));

        // explicit tag/branch/commit
        let t: InstallTarget = "o/r@tag:v1.0.0".parse().unwrap();
        let r = t.resolve().unwrap();
        matches!(r.ref_kind, crate::resolver::RefKind::Tag(_));

        let t: InstallTarget = "gitlab.com/o/r@branch:dev".parse().unwrap();
        let r = t.resolve().unwrap();
        assert_eq!(r.source, "https://gitlab.com/o/r");
        matches!(r.ref_kind, crate::resolver::RefKind::Branch(_));

        let t: InstallTarget = "https://example.com/o/r".parse().unwrap();
        let r = t.resolve().unwrap();
        assert_eq!(r.source, "https://example.com/o/r");
        matches!(r.ref_kind, crate::resolver::RefKind::None);

        // local path
        let home = std::env::var("HOME").unwrap();
        let t = InstallTarget::from_raw(home.to_string());
        let r = t.resolve().unwrap();
        assert!(r.is_local);
    }
}

#[derive(Args, Debug)]
pub(crate) struct MigrateArgs {
    /// Do not write files; print planned changes
    #[arg(long)]
    pub(crate) dry_run: bool,

    /// Overwrite existing pez.toml plugin list instead of merging
    #[arg(long)]
    pub(crate) force: bool,

    /// Immediately install migrated plugins
    #[arg(long)]
    pub(crate) install: bool,
}
