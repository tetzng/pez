use crate::lock_file::PluginFile;
use crate::models::PluginRepo;
use crate::{config, lock_file::LockFile};
use std::{fs, path};

pub(crate) struct TestEnvironmentSetup {
    pub(crate) _temp_dir: tempfile::TempDir,
    pub(crate) fish_config_dir: path::PathBuf,
    #[allow(dead_code)]
    pub(crate) config_dir: path::PathBuf,
    pub(crate) data_dir: path::PathBuf,
    pub(crate) config: Option<config::Config>,
    pub(crate) config_path: path::PathBuf,
    pub(crate) lock_file: Option<LockFile>,
    pub(crate) lock_file_path: path::PathBuf,
}

impl TestEnvironmentSetup {
    pub(crate) fn new() -> Self {
        let temp_dir = tempfile::tempdir().unwrap();
        let fish_config_dir = temp_dir.path().join("fish");
        fs::create_dir_all(&fish_config_dir).unwrap();

        let config_dir = temp_dir.path().join("pez");
        fs::create_dir_all(&config_dir).unwrap();

        let data_dir = temp_dir.path().join("data");
        fs::create_dir_all(&data_dir).unwrap();

        let config_path = config_dir.join("pez.toml");
        let lock_file_path = config_dir.join("pez-lock.toml");

        Self {
            _temp_dir: temp_dir,
            fish_config_dir,
            config_dir,
            data_dir,
            config: None,
            config_path,
            lock_file: None,
            lock_file_path,
        }
    }

    pub(crate) fn setup_config(&mut self, config: config::Config) {
        self.config = Some(config.clone());
        config.save(&self.config_path).unwrap();
    }

    pub(crate) fn setup_lock_file(&mut self, lock_file: LockFile) {
        self.lock_file = Some(lock_file.clone());
        lock_file.save(&self.lock_file_path).unwrap();
    }

    pub(crate) fn setup_data_repo(&self, repos: Vec<PluginRepo>) {
        for repo in repos {
            let repo_path = self.data_dir.join(repo.as_str());
            fs::create_dir_all(repo_path).unwrap();
        }
    }

    pub(crate) fn add_plugin_files_to_repo(&self, repo: &PluginRepo, files: &[PluginFile]) {
        let repo_path = self.data_dir.join(repo.as_str());
        for file in files {
            let dir = repo_path.join(file.dir.as_str());
            let file_path = dir.join(file.name.as_str());
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).unwrap();
            } else if !dir.exists() {
                fs::create_dir_all(&dir).unwrap();
            }
            fs::File::create(file_path).unwrap();
        }
    }

    pub(crate) fn setup_fish_config(&self) {
        self.lock_file
            .as_ref()
            .unwrap()
            .plugins
            .iter()
            .for_each(|plugin| {
                plugin.files.iter().for_each(|file| {
                    let dest_path = file.get_path(&self.fish_config_dir);
                    fs::create_dir_all(dest_path.parent().unwrap()).unwrap();
                    fs::File::create(dest_path).unwrap();
                });
            });
    }
}

impl LockFile {
    pub(crate) fn get_plugin_repos(&self) -> Vec<PluginRepo> {
        self.plugins.iter().map(|p| p.repo.clone()).collect()
    }
}

impl config::Config {
    #[allow(dead_code)]
    pub(crate) fn get_plugin_repos(&self) -> Vec<PluginRepo> {
        self.plugins
            .as_ref()
            .unwrap()
            .iter()
            .filter_map(|p| p.get_plugin_repo().ok())
            .collect()
    }
}
