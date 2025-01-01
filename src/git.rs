use git2::{Cred, Error, FetchOptions, RemoteCallbacks};
use std::path::Path;

pub(crate) fn format_git_url(plugin: &str) -> String {
    format!("https://github.com/{plugin}")
}

pub(crate) fn clone_repository(
    repo_url: &str,
    target_path: &Path,
) -> Result<git2::Repository, Error> {
    let callbacks = setup_remote_callbacks();
    let fetch_options = setup_fetch_options(callbacks);

    let mut clone_options = git2::build::RepoBuilder::new();
    clone_options.fetch_options(fetch_options);
    let repo = clone_options.clone(repo_url, target_path)?;

    Ok(repo)
}

fn setup_remote_callbacks() -> RemoteCallbacks<'static> {
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(|_, _, _| Cred::default());
    callbacks
}

fn setup_fetch_options(callbacks: RemoteCallbacks<'static>) -> FetchOptions<'static> {
    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);
    fetch_options.download_tags(git2::AutotagOption::None);
    fetch_options
}

pub(crate) fn get_latest_commit_sha(repo: git2::Repository) -> Result<String, git2::Error> {
    let commit = repo.head()?.peel_to_commit()?;

    Ok(commit.id().to_string())
}
