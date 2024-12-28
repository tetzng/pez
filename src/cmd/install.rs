use std::path::PathBuf;

use crate::{cli::InstallArgs, lock_file::LockFile, utils::copy_files_to_config};

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
    // owner/repo
    let parts = plugin_repo.split("/").collect::<Vec<&str>>();
    if parts.len() != 2 {
        eprintln!("Invalid plugin format: {}", plugin_repo);
        return;
    }

    // repo
    let name = parts[1].to_string();
    // https://github.com/owner/repo
    let source = crate::utils::format_git_url(plugin_repo);

    let pez_config_dir = crate::utils::resolve_pez_config_dir();
    if !pez_config_dir.exists() {
        std::fs::create_dir_all(&pez_config_dir).unwrap();
    }

    let pez_toml_path = pez_config_dir.join("pez.toml");

    let mut config = if std::fs::metadata(&pez_toml_path).is_ok() {
        crate::config::load(&pez_toml_path)
    } else {
        crate::config::init()
    };

    if !config.plugins.iter().any(|p| p.repo == plugin_repo) {
        config.plugins.push(crate::config::PluginSpec {
            repo: plugin_repo.to_string(),
            name: None,
            source: None,
        });
        let config_contents = toml::to_string(&config).unwrap();

        std::fs::write(&pez_toml_path, config_contents).unwrap();
    }

    // ~/.local/share/fish/pez/repo
    let repo_path = crate::utils::resolve_pez_data_dir().join(&name);

    // ~/.config/fish/pez-lock.toml
    let lock_file_path = crate::utils::resolve_lock_file_path();
    let mut lock_file = load_or_initialize_lock_file(&lock_file_path);

    // lock_file に同じpluginが存在する場合
    match lock_file.get_plugin(&source) {
        Some(locked_plugin) => {
            // clone先のディレクトリが存在する場合
            if repo_path.exists() {
                // 強制インストールの場合はアンインストールしてからインストール
                if *force {
                    // ファイルの削除
                    std::fs::remove_dir_all(&repo_path).unwrap();
                    // lock_fileから削除
                    lock_file.remove_plugin(&source);
                    // clone
                    let repo = git2::Repository::clone(&source, &repo_path).unwrap();
                    let commit_sha = crate::utils::get_latest_commit_sha(repo).unwrap();
                    let mut plugin = crate::models::Plugin {
                        name,
                        repo: plugin_repo.to_string(),
                        source,
                        commit_sha,
                        files: vec![],
                    };
                    // ファイルのコピー
                    copy_files_to_config(&repo_path, &mut plugin);
                    // lock_fileに追加
                    lock_file.add_plugin(plugin);
                    // lock_fileの内容を更新
                    let lock_file_contents = toml::to_string(&lock_file).unwrap();
                    std::fs::write(lock_file_path, lock_file_contents).unwrap();
                } else {
                    // 強制インストールでない場合はエラーメッセージを表示して終了
                    eprintln!("Plugin already exists: {}, Use --force to reinstall", name)
                }
            } else {
                // lock_file のcommit_shaをもとにclone
                let repo = git2::Repository::clone(&source, &repo_path).unwrap();
                repo.set_head_detached(git2::Oid::from_str(&locked_plugin.commit_sha).unwrap())
                    .unwrap();
                // ファイルのコピー
                let mut plugin = crate::models::Plugin {
                    name,
                    repo: plugin_repo.to_string(),
                    source,
                    commit_sha: locked_plugin.commit_sha.clone(),
                    files: vec![],
                };
                crate::utils::copy_files_to_config(&repo_path, &mut plugin);
                // lock_fileに追加
                lock_file.update_plugin(plugin);
            }
        }
        // lock_file に同じpluginが存在しない場合 cloneしてファイルをコピー
        None => {
            let repo = git2::Repository::clone(&source, &repo_path).unwrap();
            // ディレクトリがある場合は単にどのコミットにいるかを確認
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
            std::fs::write(lock_file_path, lock_file_contents).unwrap();

            println!("Files copied to config directory");
        }
    }
}

fn install_from_lock_file(force: &bool) {
    // lock_fileのパスを取得
    let lock_file_path = crate::utils::resolve_lock_file_path();
    // lock_fileをロード
    let mut lock_file = load_or_initialize_lock_file(&lock_file_path);
    // lock_fileのpluginsを取得
    // let plugins = lock_file.plugins;

    // pez.tomlをロード
    let pez_toml_path = crate::utils::resolve_pez_config_dir().join("pez.toml");
    let config = crate::config::load(&pez_toml_path);
    // pez.tomlのpluginsを取得
    let plugin_specs = config.plugins;
    // repo が pez.toml にあって pez-lock.toml にない場合は、対象のpluginを新規installする
    // repo が pez.toml にあって pez-lock.toml にもある場合は、対象のpluginをlock_fileのcommit_shaをもとにcloneしてファイルをコピーする
    // repo が pez-lock.toml にあって pez.toml にない場合は、対象のpluginを削除するには個別にunistallするか、pruneを実行する必要があることを処理の最後に表示したい

    // pluginsの数だけinstallを実行
    for plugin_spec in plugin_specs {
        let repo_path = crate::utils::resolve_pez_data_dir().join(&plugin_spec.repo);
        if repo_path.exists() {
            // 強制インストールの場合はアンインストールしてからインストール
            if *force {
                // ファイルの削除
                std::fs::remove_dir_all(&repo_path).unwrap();
                // lock_fileから削除
                let source = crate::utils::format_git_url(&plugin_spec.repo);
                lock_file.remove_plugin(&source);
                // clone
                let repo = git2::Repository::clone(&source, &repo_path).unwrap();
                let commit_sha = crate::utils::get_latest_commit_sha(repo).unwrap();
                let mut plugin = crate::models::Plugin {
                    name: plugin_spec.get_name(),
                    repo: plugin_spec.repo.to_string(),
                    source,
                    commit_sha,
                    files: vec![],
                };
                // ファイルのコピー
                copy_files_to_config(&repo_path, &mut plugin);
                // lock_fileに追加
                lock_file.add_plugin(plugin);
                // lock_fileの内容を更新
                let lock_file_contents = toml::to_string(&lock_file).unwrap();
                std::fs::write(&lock_file_path, lock_file_contents).unwrap();
                println!("Force install");
            } else {
                // 強制インストールでない場合はこのプラグインはすでにインストールされているため、インストールをスキップ
                println!(
                    "Plugin already exists: {}, Use --force to reinstall",
                    repo_path.display()
                );
            }
        } else {
            // lock_file のcommit_shaをもとにclone
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
            // ファイルのコピー
            let mut plugin = crate::models::Plugin {
                name: plugin_spec.get_name(),
                repo: plugin_spec.repo.to_string(),
                source,
                commit_sha: commit_sha.clone(),
                files: vec![],
            };
            crate::utils::copy_files_to_config(&repo_path, &mut plugin);
            // lock_fileに追加
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
