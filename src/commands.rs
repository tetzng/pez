pub(crate) fn install(path: &str) {
    let (author, repo) = crate::models::parse_author_and_repo(path);
    let git_url = crate::utils::format_git_url(author.clone(), repo.clone());
    println!("start to install {}", &git_url);

    let pez_dir = crate::utils::ensure_pez_dir();
    if !pez_dir.exists() {
        std::fs::create_dir_all(&pez_dir).unwrap();
    }
    let author_dir = pez_dir.join(author.0.as_str());
    if !author_dir.exists() {
        std::fs::create_dir_all(&author_dir).unwrap();
    }
    let repo_path = author_dir.join(repo.0.as_str());
    if repo_path.exists() {
        println!("Repository already exists");
    } else if let Err(e) = crate::utils::clone_repo(&git_url, &repo_path) {
        eprintln!("Failed to clone repository: {e}");
    } else {
        println!("Repository cloned successfully");
        let latest_commit_hash = crate::utils::get_latest_commit_hash(&repo_path).unwrap();
        let mut plugin = crate::models::Plugin {
            author,
            repo,
            source: "github.com".to_owned(),
            hash: latest_commit_hash,
            files: vec![],
        };
        crate::utils::copy_files_to_config(&repo_path, &mut plugin);
        let lock_file_path = pez_dir.join("pez-lock.toml");

        let config_dir = crate::utils::ensure_config_dir();
        let mut lock_file: crate::lockfile::LockFile;
        if !lock_file_path.exists() {
            lock_file = crate::lockfile::init_lock_file(config_dir);
        } else {
            lock_file = crate::lockfile::load_lock_file(&lock_file_path);
        };
        lock_file.plugins.push(plugin);

        let toml = toml::to_string(&lock_file).unwrap();
        std::fs::write(lock_file_path, toml).unwrap();

        println!("Files copied to config directory");
    }
}
