use crate::{cli, git, lock_file::Plugin, utils};
use anyhow::Ok;
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

    let (lock_file, _) = result.unwrap();
    let plugins = &lock_file.plugins;
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
            cli::ListFormat::Table => display_plugins_in_table(plugins),
            cli::ListFormat::Json => list_json(plugins)?,
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

fn display_plugins_in_table(plugins: &[Plugin]) {
    let plugin_rows = plugins
        .iter()
        .map(|p| PluginRow {
            name: p.get_name(),
            repo: p.repo.as_str().clone(),
            source: p.source.clone(),
            commit: p.commit_sha[..7].to_string(),
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
            let repo = git2::Repository::open(&repo_path).unwrap();
            let latest_remote_commit = git::get_latest_remote_commit(&repo).unwrap();
            p.commit_sha != latest_remote_commit
        })
        .cloned()
        .collect();

    Ok(outdated_plugins)
}

fn list_outdated_table(plugins: &[Plugin]) -> anyhow::Result<()> {
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
            current: p.commit_sha[..7].to_string(),
            latest: {
                let repo_path = data_dir.join(p.repo.as_str());
                let repo = git2::Repository::open(&repo_path).unwrap();
                git::get_latest_remote_commit(&repo).unwrap()[..7].to_string()
            },
        })
        .collect::<Vec<PluginOutdatedRow>>();
    let table = Table::new(&plugin_rows);
    println!("{table}");

    Ok(())
}

fn list_json(plugins: &[Plugin]) -> anyhow::Result<()> {
    let value = json!(
        plugins
            .iter()
            .map(|p| json!({
                "name": p.get_name(),
                "repo": p.repo.as_str(),
                "source": p.source,
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
                let repo = git2::Repository::open(&repo_path).unwrap();
                let latest = git::get_latest_remote_commit(&repo).unwrap();
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
    use cli::PluginRepo;

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
