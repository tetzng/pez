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

pub(crate) fn run(args: &crate::cli::ListArgs) {
    let lock_file_path = crate::utils::resolve_lock_file_dir().join("pez-lock.toml");
    if !lock_file_path.exists() {
        println!("No plugins installed");
        return;
    }

    let lock_file = crate::lock_file::load(&lock_file_path);

    if args.outdated {
        match args.format {
            Some(crate::cli::ListFormat::Table) => list_outdated_table(lock_file),
            None => list_outdated(lock_file),
        }
    } else {
        match args.format {
            Some(crate::cli::ListFormat::Table) => list_table(lock_file),
            None => list(lock_file),
        }
    }
}

fn list(lock_file: crate::lock_file::LockFile) {
    for plugin in &lock_file.plugins {
        println!("{}", plugin.repo,);
    }
}

fn list_table(lock_file: crate::lock_file::LockFile) {
    let plugins = lock_file
        .plugins
        .iter()
        .map(|p| PluginRow {
            name: p.get_name(),
            repo: p.repo.clone(),
            source: p.source.clone(),
            commit: p.commit_sha[..7].to_string(),
        })
        .collect::<Vec<PluginRow>>();
    let table = Table::new(&plugins);
    println!("{table}");
}

fn list_outdated(lock_file: crate::lock_file::LockFile) {
    let plugins = lock_file.plugins.iter().filter(|p| {
        let repo_path = get_repo_path(&p.repo);
        let repo = git2::Repository::open(&repo_path).unwrap();
        let latest_remote_commit = crate::git::get_latest_remote_commit(&repo).unwrap();
        p.commit_sha != latest_remote_commit
    });
    if plugins.clone().count() == 0 {
        println!("{}All plugins are up to date!", Emoji("🎉 ", ""));
        return;
    }
    plugins.for_each(|p| {
        println!("{}", p.repo);
    });
}

fn list_outdated_table(lock_file: crate::lock_file::LockFile) {
    let plugins = lock_file
        .plugins
        .iter()
        .filter(|p| {
            let repo_path = get_repo_path(&p.repo);
            let repo = git2::Repository::open(&repo_path).unwrap();
            let latest_remote_commit = crate::git::get_latest_remote_commit(&repo).unwrap();
            p.commit_sha != latest_remote_commit
        })
        .map(|p| PluginOutdatedRow {
            name: p.get_name(),
            repo: p.repo.clone(),
            source: p.source.clone(),
            current: p.commit_sha[..7].to_string(),
            latest: {
                let repo_path = get_repo_path(&p.repo);
                let repo = git2::Repository::open(&repo_path).unwrap();
                crate::git::get_latest_remote_commit(&repo).unwrap()[..7].to_string()
            },
        })
        .collect::<Vec<PluginOutdatedRow>>();
    let table = Table::new(&plugins);
    println!("{table}");
}

fn get_repo_path(plugin_repo: &str) -> std::path::PathBuf {
    crate::utils::resolve_pez_data_dir().join(plugin_repo)
}
