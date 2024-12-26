use std::{env, path::PathBuf};

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Install a fish plugin
    Install {
        /// GitHub repo in the format <author>/<repo>
        path: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Install { path } => {
            let (author, repo) = parse_author_and_repo(path);
            let git_url = format_git_url(author.clone(), repo.clone());
            println!("start to install {}", &git_url);

            let pez_dir = ensure_pez_dir();
            if !pez_dir.exists() {
                std::fs::create_dir_all(&pez_dir).unwrap();
            }
            let author_dir = pez_dir.join(author.0.as_str());
            if !author_dir.exists() {
                std::fs::create_dir_all(&author_dir).unwrap();
            }
            let repo_dir = author_dir.join(repo.0.as_str());
            if repo_dir.exists() {
                println!("Repository already exists");
            } else if let Err(e) = clone_repo(&git_url, &repo_dir) {
                eprintln!("Failed to clone repository: {e}");
            } else {
                println!("Repository cloned successfully");
                copy_files_to_config(&repo_dir);
                println!("Files copied to config directory");
            }
        }
    }
}

fn clone_repo(url: &str, path: &std::path::Path) -> Result<(), git2::Error> {
    let mut opts = git2::FetchOptions::new();
    opts.download_tags(git2::AutotagOption::All);
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(opts);
    builder.clone(url, path)?;

    Ok(())
}

#[derive(Clone, Debug)]
struct Author(String);

#[derive(Clone, Debug)]
struct Repo(String);

// author/repoの文字列から、authorとrepoに分割して返す
// returnがAuthor型とRepo型になるようにする
fn parse_author_and_repo(path: &str) -> (Author, Repo) {
    let parts = path.split('/').collect::<Vec<&str>>();
    if parts.len() != 2 {
        panic!("Invalid repository path");
    }
    (Author(parts[0].to_string()), Repo(parts[1].to_string()))
}

fn format_git_url(author: Author, repo: Repo) -> String {
    format!("https://github.com/{}/{}.git", author.0, repo.0)
}

fn ensure_pez_dir() -> PathBuf {
    if let Some(dir) = env::var_os("__fish_user_data_dir") {
        return PathBuf::from(dir).join("pez");
    }
    if let Some(dir) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(dir).join("fish").join("pez");
    }
    let home = env::var("HOME").unwrap();
    PathBuf::from(home).join(".local/share/fish/pez")
}

fn copy_files_to_config(repo_dir: &std::path::Path) {
    let config_dir = ensure_config_dir();
    let target_dirs = vec!["functions", "completions", "conf.d", "themes"];

    for target_dir in target_dirs {
        let target_dir = repo_dir.join(target_dir);
        if !target_dir.exists() {
            continue;
        }
        let dest_dir = config_dir.join(target_dir.file_name().unwrap());
        if !dest_dir.exists() {
            std::fs::create_dir_all(&dest_dir).unwrap();
            println!("Created directory: {}", dest_dir.display());
        }
        let files = std::fs::read_dir(target_dir).unwrap();
        println!("Copying files to {}", dest_dir.display());
        for file in files {
            let file = file.unwrap();
            if file.file_type().unwrap().is_dir() {
                continue;
            }
            let file_name = file.file_name();
            let file_path = file.path();
            let dest_path = dest_dir.join(&file_name);
            std::fs::copy(file_path, &dest_path).unwrap();
            println!("Copied: {}", dest_path.display());
        }
    }
}

fn ensure_config_dir() -> PathBuf {
    if let Some(dir) = env::var_os("__fish_config_dir") {
        return PathBuf::from(dir);
    }
    if let Some(dir) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(dir).join("fish");
    }
    let home = env::var("HOME").unwrap();
    PathBuf::from(home).join(".config/fish")
}
