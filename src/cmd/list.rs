use crate::{cli, git, lock_file::Plugin, utils};

use console::Emoji;
use serde_json::json;
use std::io;
use tabled::{Table, Tabled};
use tracing::info;

#[derive(Debug, Tabled)]
struct PluginRow {
    name: String,
    repo: String,
    source: String,
    selector: String,
    commit: String,
}

#[derive(Debug, Tabled)]
struct PluginOutdatedRow {
    name: String,
    repo: String,
    source: String,
    current: String,
    latest: String,
}

pub(crate) fn run(args: &cli::ListArgs) -> anyhow::Result<()> {
    let result = utils::load_lock_file();
    if result.is_err() {
        info!("No plugins installed!");
        return Ok(());
    }

    let config_opt = utils::load_config().ok().map(|(c, _)| c);
    let (lock_file, _) = match result {
        Ok(v) => v,
        Err(_) => {
            info!("No plugins installed!");
            return Ok(());
        }
    };
    let mut plugins = lock_file.plugins.clone();
    if let Some(filter) = &args.filter {
        match filter {
            cli::ListFilter::All => {}
            cli::ListFilter::Local => plugins.retain(|p| git::is_local_source(&p.source)),
            cli::ListFilter::Remote => plugins.retain(|p| !git::is_local_source(&p.source)),
        }
    }
    let plugins = &plugins;
    if plugins.is_empty() {
        info!("No plugins installed!");
        return Ok(());
    }

    if args.outdated {
        match args.format.clone().unwrap_or(cli::ListFormat::Plain) {
            cli::ListFormat::Table => list_outdated_table(plugins)?,
            cli::ListFormat::Json => list_outdated_json(plugins)?,
            cli::ListFormat::Plain => list_outdated(plugins)?,
        }
    } else {
        match args.format.clone().unwrap_or(cli::ListFormat::Plain) {
            cli::ListFormat::Table => display_plugins_in_table(plugins, config_opt.as_ref()),
            cli::ListFormat::Json => list_json(plugins, config_opt.as_ref())?,
            cli::ListFormat::Plain => list(plugins)?,
        }
    }

    Ok(())
}

fn list(plugins: &[Plugin]) -> anyhow::Result<()> {
    display_plugins(plugins, io::stdout())?;
    Ok(())
}

fn display_plugins<W: io::Write>(plugins: &[Plugin], mut writer: W) -> anyhow::Result<()> {
    for plugin in plugins {
        writeln!(writer, "{}", plugin.repo)?;
    }

    Ok(())
}

fn display_plugins_in_table(plugins: &[Plugin], config: Option<&crate::config::Config>) {
    fn short7(s: &str) -> String {
        s.chars().take(7).collect()
    }
    fn selector_of(
        cfg: Option<&crate::config::Config>,
        repo: &crate::models::PluginRepo,
    ) -> String {
        let cfg = match cfg {
            Some(c) => c,
            None => return "-".into(),
        };
        let spec = match cfg.plugins.as_ref().and_then(|ps| {
            ps.iter()
                .find(|p| p.get_plugin_repo().ok().as_ref() == Some(repo))
        }) {
            Some(s) => s,
            None => return "-".into(),
        };
        match &spec.source {
            crate::config::PluginSource::Repo {
                version,
                branch,
                tag,
                commit,
                ..
            }
            | crate::config::PluginSource::Url {
                version,
                branch,
                tag,
                commit,
                ..
            } => {
                if let Some(c) = commit {
                    return format!("commit:{}", c);
                }
                if let Some(b) = branch {
                    return format!("branch:{}", b);
                }
                if let Some(t) = tag {
                    return format!("tag:{}", t);
                }
                if let Some(v) = version {
                    return format!("version:{}", v);
                }
                "-".into()
            }
            crate::config::PluginSource::Path { .. } => "local".into(),
        }
    }
    let plugin_rows = plugins
        .iter()
        .map(|p| PluginRow {
            name: p.get_name(),
            repo: p.repo.as_str().clone(),
            source: p.source.clone(),
            selector: selector_of(config, &p.repo),
            commit: short7(&p.commit_sha),
        })
        .collect::<Vec<PluginRow>>();
    let table = Table::new(&plugin_rows);
    println!("{table}");
}

fn list_outdated(plugins: &[Plugin]) -> anyhow::Result<()> {
    let outdated_plugins = get_outdated_plugins(plugins)?;
    if outdated_plugins.is_empty() {
        info!("{}All plugins are up to date!", Emoji("🎉 ", ""));
        return Ok(());
    }
    display_plugins(&outdated_plugins, io::stdout())?;

    Ok(())
}

fn get_outdated_plugins(plugins: &[Plugin]) -> anyhow::Result<Vec<Plugin>> {
    let data_dir = utils::load_pez_data_dir()?;
    let outdated_plugins: Vec<Plugin> = plugins
        .iter()
        .filter(|p| {
            let repo_path = data_dir.join(p.repo.as_str());
            let latest_remote_commit = match git2::Repository::open(&repo_path)
                .ok()
                .and_then(|r| git::get_latest_remote_commit(&r).ok())
            {
                Some(s) => s,
                None => return false,
            };
            p.commit_sha != latest_remote_commit
        })
        .cloned()
        .collect();

    Ok(outdated_plugins)
}

fn list_outdated_table(plugins: &[Plugin]) -> anyhow::Result<()> {
    fn short7(s: &str) -> String {
        s.chars().take(7).collect()
    }
    let data_dir = utils::load_pez_data_dir()?;
    let outdated_plugins = get_outdated_plugins(plugins)?;
    if outdated_plugins.is_empty() {
        info!("{}All plugins are up to date!", Emoji("🎉 ", ""));
        return Ok(());
    }

    let plugin_rows = outdated_plugins
        .iter()
        .map(|p| PluginOutdatedRow {
            name: p.get_name(),
            repo: p.repo.as_str().clone(),
            source: p.source.clone(),
            current: short7(&p.commit_sha),
            latest: {
                let repo_path = data_dir.join(p.repo.as_str());
                match git2::Repository::open(&repo_path)
                    .ok()
                    .and_then(|r| git::get_latest_remote_commit(&r).ok())
                {
                    Some(s) => short7(&s),
                    None => "—".to_string(),
                }
            },
        })
        .collect::<Vec<PluginOutdatedRow>>();
    let table = Table::new(&plugin_rows);
    println!("{table}");

    Ok(())
}

fn list_json(plugins: &[Plugin], config: Option<&crate::config::Config>) -> anyhow::Result<()> {
    fn selector_of(
        cfg: Option<&crate::config::Config>,
        repo: &crate::models::PluginRepo,
    ) -> Option<String> {
        let cfg = cfg?;
        let spec = cfg.plugins.as_ref().and_then(|ps| {
            ps.iter()
                .find(|p| p.get_plugin_repo().ok().as_ref() == Some(repo))
        })?;
        match &spec.source {
            crate::config::PluginSource::Repo {
                version,
                branch,
                tag,
                commit,
                ..
            }
            | crate::config::PluginSource::Url {
                version,
                branch,
                tag,
                commit,
                ..
            } => {
                if let Some(c) = commit {
                    return Some(format!("commit:{}", c));
                }
                if let Some(b) = branch {
                    return Some(format!("branch:{}", b));
                }
                if let Some(t) = tag {
                    return Some(format!("tag:{}", t));
                }
                if let Some(v) = version {
                    return Some(format!("version:{}", v));
                }
                None
            }
            crate::config::PluginSource::Path { .. } => Some("local".into()),
        }
    }
    let value = json!(
        plugins
            .iter()
            .map(|p| json!({
                "name": p.get_name(),
                "repo": p.repo.as_str(),
                "source": p.source,
                "selector": selector_of(config, &p.repo),
                "commit": p.commit_sha,
            }))
            .collect::<Vec<_>>()
    );
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn list_outdated_json(plugins: &[Plugin]) -> anyhow::Result<()> {
    let data_dir = utils::load_pez_data_dir()?;
    let outdated_plugins = get_outdated_plugins(plugins)?;
    if outdated_plugins.is_empty() {
        info!("{}All plugins are up to date!", Emoji("🎉 ", ""));
        return Ok(());
    }
    let value = json!(
        outdated_plugins
            .iter()
            .map(|p| {
                let repo_path = data_dir.join(p.repo.as_str());
                let latest = git2::Repository::open(&repo_path)
                    .ok()
                    .and_then(|r| git::get_latest_remote_commit(&r).ok())
                    .unwrap_or_default();
                json!({
                    "name": p.get_name(),
                    "repo": p.repo.as_str(),
                    "source": p.source,
                    "current": p.commit_sha,
                    "latest": latest,
                })
            })
            .collect::<Vec<_>>()
    );
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock_file::Plugin;
    use crate::models::PluginRepo;

    #[test]
    fn test_display_plugins() {
        let plugins = vec![
            Plugin {
                name: "name".to_string(),
                repo: PluginRepo {
                    owner: "owner".to_string(),
                    repo: "repo".to_string(),
                },
                source: "source".to_string(),
                commit_sha: "commit_sha".to_string(),
                files: vec![],
            },
            Plugin {
                name: "name2".to_string(),
                repo: PluginRepo {
                    owner: "owner".to_string(),
                    repo: "repo2".to_string(),
                },
                source: "source2".to_string(),
                commit_sha: "commit_sha2".to_string(),
                files: vec![],
            },
        ];

        let mut output = io::Cursor::new(Vec::new());
        display_plugins(&plugins, &mut output).unwrap();

        let actual_output = String::from_utf8(output.into_inner()).unwrap();
        let expected_output = "owner/repo\nowner/repo2\n";

        assert_eq!(actual_output, expected_output);
    }
}
