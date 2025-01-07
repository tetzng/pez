use crate::{
    cli::{PluginRepo, UpgradeArgs},
    config::PluginSpec,
    git,
    lock_file::Plugin,
    utils,
};
use console::Emoji;
use std::{fs, process};

pub(crate) fn run(args: &UpgradeArgs) {
    println!("{}Starting upgrade process...", Emoji("🔍 ", ""));
    if let Some(plugins) = &args.plugins {
        for plugin in plugins {
            println!("\n{}Upgrading plugin: {plugin}", Emoji("✨ ", ""));
            upgrade(plugin);
            println!(
                "{}Successfully upgraded plugin: {}",
                Emoji("✅ ", ""),
                plugin
            );
        }
    } else {
        upgrade_all();
    }
    println!(
        "\n{}All specified plugins have been upgraded successfully!",
        Emoji("🎉 ", "")
    );
}

fn upgrade(plugin: &PluginRepo) {
    let plugin = plugin.as_str();
    let (mut config, config_path) = utils::ensure_config();

    match config.plugins {
        Some(ref mut plugin_specs) => {
            if !plugin_specs.iter().any(|p| p.repo == plugin) {
                plugin_specs.push(PluginSpec {
                    repo: plugin.to_string().clone(),
                    name: None,
                    source: None,
                });
                config.save(&config_path);
            }
        }
        None => {
            config.plugins = Some(vec![PluginSpec {
                repo: plugin.to_string(),
                name: None,
                source: None,
            }]);
            config.save(&config_path);
        }
    }

    upgrade_plugin(&plugin);
}

fn upgrade_all() {
    let (config, _) = utils::ensure_config();
    if let Some(plugins) = &config.plugins {
        for plugin in plugins {
            println!("\n{}Upgrading plugin: {}", Emoji("✨ ", ""), &plugin.repo);
            upgrade_plugin(&plugin.repo);
        }
    }
}

fn upgrade_plugin(plugin_repo: &str) {
    let (mut lock_file, lock_file_path) = utils::ensure_lock_file();
    let source = &git::format_git_url(plugin_repo);
    let config_dir = utils::resolve_fish_config_dir();

    match lock_file.get_plugin(source) {
        Some(lock_file_plugin) => {
            let repo_path = utils::resolve_pez_data_dir().join(&lock_file_plugin.repo);
            if repo_path.exists() {
                let repo = git2::Repository::open(&repo_path).unwrap();
                let latest_remote_commit = git::get_latest_remote_commit(&repo).unwrap();
                if latest_remote_commit == lock_file_plugin.commit_sha {
                    println!(
                        "{}{} Plugin {} is already up to date.",
                        Emoji("🚀 ", ""),
                        console::style("Info:").cyan(),
                        plugin_repo
                    );
                    return;
                }

                repo.set_head_detached(git2::Oid::from_str(&latest_remote_commit).unwrap())
                    .unwrap();

                lock_file_plugin.files.iter().for_each(|file| {
                    let dest_path = config_dir.join(file.dir.as_str()).join(&file.name);
                    if dest_path.exists() {
                        fs::remove_file(&dest_path).unwrap();
                    }
                });
                let mut updated_plugin = Plugin {
                    name: lock_file_plugin.name.to_string(),
                    repo: plugin_repo.to_string(),
                    source: source.to_string(),
                    commit_sha: latest_remote_commit,
                    files: vec![],
                };
                println!("{:?}", updated_plugin);

                utils::copy_files_to_config(&repo_path, &mut updated_plugin);

                lock_file.update_plugin(updated_plugin);
                lock_file.save(&lock_file_path);
            } else {
                println!(
                    "{}{} Repository directory at {} does not exist.",
                    Emoji("🚧 ", ""),
                    console::style("Warning:").yellow(),
                    &repo_path.display()
                );
                println!("{}You need to install the plugin first.", Emoji("🚧 ", ""),);
            }
        }
        None => {
            eprintln!(
                "{}{} Plugin {} is not installed.",
                Emoji("❌ ", ""),
                console::style("Error:").red().bold(),
                plugin_repo
            );
            process::exit(1);
        }
    }
}
