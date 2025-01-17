use crate::{cli, git, lock_file::Plugin, utils};
use anyhow::Ok;
use console::Emoji;
use tabled::{Table, Tabled};

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
        println!("No plugins installed!");
        return Ok(());
    }

    let (lock_file, _) = result.unwrap();
    let plugins = &lock_file.plugins;
    if plugins.is_empty() {
        println!("No plugins installed!");
        return Ok(());
    }

    if args.outdated {
        match args.format {
            Some(cli::ListFormat::Table) => list_outdated_table(plugins)?,
            None => list_outdated(plugins)?,
        }
    } else {
        match args.format {
            Some(cli::ListFormat::Table) => display_plugins_in_table(plugins),
            None => list(plugins),
        }
    }

    Ok(())
}

fn list(plugins: &[Plugin]) {
    display_plugins(plugins);
}

fn display_plugins(plugins: &[Plugin]) {
    plugins.iter().for_each(|p| {
        println!("{}", p.repo);
    });
}

fn display_plugins_in_table(plugins: &[Plugin]) {
    let plugin_rows = plugins
        .iter()
        .map(|p| PluginRow {
            name: p.get_name(),
            repo: p.repo.clone(),
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
        println!("{}All plugins are up to date!", Emoji("🎉 ", ""));
        return Ok(());
    }
    display_plugins(&outdated_plugins);

    Ok(())
}

fn get_outdated_plugins(plugins: &[Plugin]) -> anyhow::Result<Vec<Plugin>> {
    let data_dir = utils::load_pez_data_dir()?;
    let outdated_plugins: Vec<Plugin> = plugins
        .iter()
        .filter(|p| {
            let repo_path = data_dir.join(&p.repo);
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
        println!("{}All plugins are up to date!", Emoji("🎉 ", ""));
        return Ok(());
    }

    let plugin_rows = outdated_plugins
        .iter()
        .map(|p| PluginOutdatedRow {
            name: p.get_name(),
            repo: p.repo.clone(),
            source: p.source.clone(),
            current: p.commit_sha[..7].to_string(),
            latest: {
                let repo_path = data_dir.join(&p.repo);
                let repo = git2::Repository::open(&repo_path).unwrap();
                git::get_latest_remote_commit(&repo).unwrap()[..7].to_string()
            },
        })
        .collect::<Vec<PluginOutdatedRow>>();
    let table = Table::new(&plugin_rows);
    println!("{table}");

    Ok(())
}
