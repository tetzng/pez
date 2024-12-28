use sha2::{Digest, Sha256};
use std::fs;
use std::{env, path::PathBuf};

pub(crate) fn clone_repo(url: &str, path: &std::path::Path) -> Result<(), git2::Error> {
    let mut opts = git2::FetchOptions::new();
    opts.download_tags(git2::AutotagOption::All);
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(opts);
    builder.clone(url, path)?;

    Ok(())
}

pub(crate) fn get_latest_commit_hash(repo_path: &std::path::Path) -> Result<String, git2::Error> {
    let repo = git2::Repository::open(repo_path)?;
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;
    Ok(commit.id().to_string())
}

pub(crate) fn format_git_url(author: crate::models::Author, repo: crate::models::Repo) -> String {
    format!("https://github.com/{}/{}.git", author.0, repo.0)
}

pub(crate) fn ensure_pez_dir() -> PathBuf {
    if let Some(dir) = env::var_os("__fish_user_data_dir") {
        return PathBuf::from(dir).join("pez");
    }
    if let Some(dir) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(dir).join("fish").join("pez");
    }
    let home = env::var("HOME").unwrap();
    PathBuf::from(home).join(".local/share/fish/pez")
}

pub(crate) fn copy_files_to_config(repo_dir: &std::path::Path, plugin: &mut crate::models::Plugin) {
    let config_dir = ensure_config_dir();
    let target_dirs = vec![
        crate::models::TargetDir::Functions,
        crate::models::TargetDir::Completions,
        crate::models::TargetDir::ConfD,
        crate::models::TargetDir::Themes,
    ];
    let mut has_target_file = false;

    for target_dir in target_dirs {
        let target_path = repo_dir.join(target_dir.as_str());
        if !target_path.exists() {
            continue;
        }
        if !has_target_file {
            has_target_file = true;
        }
        let dest_path = config_dir.join(target_path.file_name().unwrap());
        if !dest_path.exists() {
            std::fs::create_dir_all(&dest_path).unwrap();
            println!("Created directory: {}", dest_path.display());
        }
        let files = std::fs::read_dir(target_path).unwrap();
        println!("Copying files to {}", dest_path.display());
        for file in files {
            let file = file.unwrap();
            if file.file_type().unwrap().is_dir() {
                continue;
            }
            let file_name = file.file_name();
            let file_path = file.path();
            let dest_file_path = dest_path.join(&file_name);
            std::fs::copy(&file_path, &dest_file_path).unwrap();
            println!("Copied: {}", dest_file_path.display());
            let plugin_file = crate::models::PluginFile {
                dir: target_dir.clone(),
                name: file_name.to_string_lossy().to_string(),
                hash: calculate_file_hash(&file_path),
            };
            plugin.files.push(plugin_file);
        }
    }
    if !has_target_file {
        println!("No target files found");
    }
}

pub(crate) fn ensure_config_dir() -> PathBuf {
    if let Some(dir) = env::var_os("__fish_config_dir") {
        return PathBuf::from(dir);
    }
    if let Some(dir) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(dir).join("fish");
    }
    let home = env::var("HOME").unwrap();
    PathBuf::from(home).join(".config/fish")
}

pub(crate) fn calculate_file_hash(path: &std::path::PathBuf) -> String {
    let content = fs::read(path).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}
