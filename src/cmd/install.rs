use std::path::PathBuf;

use crate::{
    cli::InstallArgs,
    config::PluginSpec,
    lock_file::{LockFile, AUTO_GENERATED_COMMENT},
    utils::copy_files_to_config,
};

pub(crate) fn run(args: &InstallArgs) {
    if let Some(plugins) = &args.plugins {
        for plugin in plugins {
            install(plugin, &args.force);
        }
    } else {
        install_from_lock_file(&args.force);
    }
}

fn install(plugin_repo: &str, force: &bool) {
    let parts = plugin_repo.split("/").collect::<Vec<&str>>();
    if parts.len() != 2 {
        eprintln!("Invalid plugin format: {}", plugin_repo);
        return;
    }

    let name = parts[1].to_string();
    let source = crate::utils::format_git_url(plugin_repo);

    let pez_config_dir = crate::utils::resolve_pez_config_dir();
    if !pez_config_dir.exists() {
        std::fs::create_dir_all(&pez_config_dir).unwrap();
    }

    let pez_toml_path = pez_config_dir.join("pez.toml");

    let mut config = if pez_toml_path.exists() {
        crate::config::load(&pez_toml_path)
    } else {
        crate::config::init()
    };

    match config.plugins {
        Some(ref mut plugin_vec) => {
            if !plugin_vec.iter().any(|p| p.repo == plugin_repo) {
                plugin_vec.push(PluginSpec {
                    repo: plugin_repo.to_string(),
                    name: None,
                    source: None,
                });
                let config_contents = toml::to_string(&config).unwrap();
                std::fs::write(&pez_toml_path, config_contents).unwrap();
            }
        }

        None => {
            config.plugins = Some(vec![PluginSpec {
                repo: plugin_repo.to_string(),
                name: None,
                source: None,
            }]);
            let config_contents = toml::to_string(&config).unwrap();
            std::fs::write(&pez_toml_path, config_contents).unwrap();
        }
    }

    let repo_path = crate::utils::resolve_pez_data_dir().join(&name);

    let lock_file_path = crate::utils::resolve_lock_file_path();
    let mut lock_file = load_or_initialize_lock_file(&lock_file_path);

    match lock_file.get_plugin(&source) {
        Some(locked_plugin) => {
            if repo_path.exists() {
                if *force {
                    std::fs::remove_dir_all(&repo_path).unwrap();
                    lock_file.remove_plugin(&source);
                    let repo = git2::Repository::clone(&source, &repo_path).unwrap();
                    let commit_sha = crate::utils::get_latest_commit_sha(repo).unwrap();
                    let mut plugin = crate::models::Plugin {
                        name,
                        repo: plugin_repo.to_string(),
                        source,
                        commit_sha,
                        files: vec![],
                    };
                    copy_files_to_config(&repo_path, &mut plugin);
                    lock_file.add_plugin(plugin);
                    let lock_file_contents = toml::to_string(&lock_file).unwrap();
                    std::fs::write(
                        lock_file_path,
                        AUTO_GENERATED_COMMENT.to_string() + &lock_file_contents,
                    )
                    .unwrap();
                } else {
                    eprintln!("Plugin already exists: {}, Use --force to reinstall", name)
                }
            } else {
                let repo = git2::Repository::clone(&source, &repo_path).unwrap();
                repo.set_head_detached(git2::Oid::from_str(&locked_plugin.commit_sha).unwrap())
                    .unwrap();
                let mut plugin = crate::models::Plugin {
                    name,
                    repo: plugin_repo.to_string(),
                    source,
                    commit_sha: locked_plugin.commit_sha.clone(),
                    files: vec![],
                };
                crate::utils::copy_files_to_config(&repo_path, &mut plugin);
                lock_file.update_plugin(plugin);
            }
        }
        None => {
            if repo_path.exists() {
                if *force {
                    std::fs::remove_dir_all(&repo_path).unwrap();
                } else {
                    eprintln!("Plugin already exists: {}, Use --force to reinstall", name)
                }
            }
            let repo = git2::Repository::clone(&source, &repo_path).unwrap();
            let commit_sha = crate::utils::get_latest_commit_sha(repo).unwrap();
            let mut plugin = crate::models::Plugin {
                name,
                repo: plugin_repo.to_string(),
                source,
                commit_sha,
                files: vec![],
            };
            crate::utils::copy_files_to_config(&repo_path, &mut plugin);

            lock_file.add_plugin(plugin);

            let lock_file_contents = toml::to_string(&lock_file).unwrap();
            std::fs::write(
                lock_file_path,
                AUTO_GENERATED_COMMENT.to_string() + &lock_file_contents,
            )
            .unwrap();

            println!("Files copied to config directory");
        }
    }
}

fn install_from_lock_file(force: &bool) {
    let lock_file_path = crate::utils::resolve_lock_file_path();
    let mut lock_file = load_or_initialize_lock_file(&lock_file_path);

    let pez_toml_path = crate::utils::resolve_pez_config_dir().join("pez.toml");
    let config = crate::config::load(&pez_toml_path);
    let plugin_specs = match config.plugins {
        Some(plugins) => plugins,
        None => {
            println!("No plugins found in pez.toml");
            vec![]
        }
    };

    for plugin_spec in plugin_specs.iter() {
        let repo_path = crate::utils::resolve_pez_data_dir().join(&plugin_spec.repo);
        if repo_path.exists() {
            if *force {
                std::fs::remove_dir_all(&repo_path).unwrap();
                let source = crate::utils::format_git_url(&plugin_spec.repo);
                lock_file.remove_plugin(&source);
                let repo = git2::Repository::clone(&source, &repo_path).unwrap();
                let commit_sha = crate::utils::get_latest_commit_sha(repo).unwrap();
                let mut plugin = crate::models::Plugin {
                    name: plugin_spec.get_name(),
                    repo: plugin_spec.repo.to_string(),
                    source,
                    commit_sha,
                    files: vec![],
                };
                copy_files_to_config(&repo_path, &mut plugin);
                lock_file.add_plugin(plugin);
                let lock_file_contents = toml::to_string(&lock_file).unwrap();
                std::fs::write(
                    &lock_file_path,
                    AUTO_GENERATED_COMMENT.to_string() + &lock_file_contents,
                )
                .unwrap();
                println!("Force install");
            } else {
                println!(
                    "Plugin already exists: {}, Use --force to reinstall",
                    repo_path.display()
                );
            }
        } else {
            let source = crate::utils::format_git_url(&plugin_spec.repo);
            let commit_sha = lock_file
                .get_plugin(&source)
                .map(|locked_plugin| locked_plugin.commit_sha.clone())
                .unwrap_or_else(|| {
                    let temp_repo = git2::Repository::clone(&source, &repo_path).unwrap();
                    crate::utils::get_latest_commit_sha(temp_repo).unwrap()
                });
            let repo = git2::Repository::clone(&source, &repo_path).unwrap();
            repo.set_head_detached(git2::Oid::from_str(&commit_sha).unwrap())
                .unwrap();
            let mut plugin = crate::models::Plugin {
                name: plugin_spec.get_name(),
                repo: plugin_spec.repo.to_string(),
                source,
                commit_sha: commit_sha.clone(),
                files: vec![],
            };
            crate::utils::copy_files_to_config(&repo_path, &mut plugin);
            lock_file.update_plugin(plugin);
        }
    }
}

fn load_or_initialize_lock_file(path: &PathBuf) -> LockFile {
    if !path.exists() {
        crate::lock_file::init()
    } else {
        crate::lock_file::load(path)
    }
}
