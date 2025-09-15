use clap::{Args, Parser, Subcommand};
// keep derives in case of future clap value types
#[allow(unused_imports)]
use serde_derive::{Deserialize, Serialize};

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
    pub(crate) plugins: Option<Vec<crate::models::InstallTarget>>,

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
    pub(crate) plugins: Option<Vec<crate::models::PluginRepo>>,

    /// Force uninstall even if the plugin data directory does not exist
    #[arg(short, long)]
    pub(crate) force: bool,
}

#[derive(Args, Debug)]
pub(crate) struct UpgradeArgs {
    /// GitHub repo in the format `owner/repo`
    pub(crate) plugins: Option<Vec<crate::models::PluginRepo>>,
}

#[derive(Args, Debug)]
pub(crate) struct ListArgs {
    /// List format
    #[arg(long, value_enum)]
    pub(crate) format: Option<ListFormat>,

    /// Show only outdated plugins
    #[arg(long)]
    pub(crate) outdated: bool,

    /// Filter plugins by source kind
    #[arg(long, value_enum)]
    pub(crate) filter: Option<ListFilter>,
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

#[derive(Debug, Clone, clap::ValueEnum)]
pub(crate) enum ListFilter {
    All,
    Local,
    Remote,
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

// Types moved to models.rs: PluginRepo, InstallTarget, ResolvedInstallTarget

use crate::models::{InstallTarget, PluginRepo, ResolvedInstallTarget};

impl InstallTarget {
    /// Create an InstallTarget from a raw string.
    pub fn from_raw<S: Into<String>>(s: S) -> Self {
        InstallTarget { raw: s.into() }
    }
    /// Parse the raw string into a `ResolvedInstallTarget`.
    /// Rules:
    /// - `owner/repo[@ref]` => github.com
    /// - `host/owner/repo[@ref]` (no scheme) => <https://host/owner/repo>
    /// - URLs with scheme left as-is (no `@ref` parsing to avoid ssh user@ conflicts)
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
            let mut path_str = expand_tilde(raw)?;
            // Normalize relative paths (./, ../, .) to absolute using current_dir
            if path_str == "." || path_str.starts_with("./") || path_str.starts_with("../") {
                let abs = std::env::current_dir()
                    .context("Failed to read current working directory")?
                    .join(&path_str);
                path_str = abs.to_string_lossy().to_string();
            }
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

        // relative path should be normalized to absolute
        let t = InstallTarget::from_raw("./some/dir");
        let r = t.resolve().unwrap();
        assert!(r.is_local);
        let cwd = std::env::current_dir().unwrap();
        assert!(r.source.starts_with(&*cwd.to_string_lossy()));
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
