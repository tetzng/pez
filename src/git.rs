use git2::{Cred, Error, FetchOptions, RemoteCallbacks};
use std::path;

pub(crate) fn format_git_url(plugin: &str) -> String {
    format!("https://github.com/{plugin}")
}

pub(crate) fn clone_repository(
    repo_url: &str,
    target_path: &path::Path,
) -> anyhow::Result<git2::Repository> {
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

fn get_remote_name(upstream: &git2::Branch) -> Result<String, Error> {
    let upstream_ref = upstream
        .get()
        .name()
        .ok_or_else(|| git2::Error::from_str("Upstream branch has no name"))?;
    let parts: Vec<&str> = upstream_ref.split('/').collect();
    if parts.len() < 3 {
        return Err(Error::from_str(&format!(
            "Invalid upstream reference format: {upstream_ref}"
        )));
    }
    Ok(parts[2].to_string())
}

pub(crate) fn get_latest_remote_commit(repo: &git2::Repository) -> anyhow::Result<String> {
    let head = repo.head()?;

    if head.is_branch() {
        let branch_name = head
            .shorthand()
            .ok_or_else(|| git2::Error::from_str("Invalid branch name"))?;

        let local_branch = repo.find_branch(branch_name, git2::BranchType::Local)?;

        let upstream = match local_branch.upstream() {
            Ok(u) => u,
            Err(_) => return Err(anyhow::anyhow!("No upstream branch set")),
        };

        let remote_name = get_remote_name(&upstream)?;

        let mut remote = repo.find_remote(&remote_name)?;

        let mut cb = RemoteCallbacks::new();
        cb.credentials(|_url, username, _allowed| {
            if let Some(username) = username {
                Cred::ssh_key_from_agent(username)
            } else {
                Err(git2::Error::from_str("No username provided"))
            }
        });

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(cb);

        remote.fetch(
            &["refs/heads/*:refs/remotes/origin/*"],
            Some(&mut fetch_options),
            None,
        )?;

        let remote_branch_ref = format!("refs/remotes/{remote_name}/{branch_name}");
        let remote_ref = match repo.find_reference(&remote_branch_ref) {
            Ok(r) => r.resolve()?,
            Err(_) => {
                let err_msg = format!("Remote branch '{remote_branch_ref}' does not exist");
                return Err(anyhow::anyhow!(err_msg));
            }
        };

        let remote_oid = match remote_ref.target() {
            Some(oid) => oid,
            None => return Err(anyhow::anyhow!("Remote branch has no target")),
        };

        let remote_commit = repo.find_commit(remote_oid)?;

        Ok(remote_commit.id().to_string())
    } else {
        let remote_name = "origin";

        let mut remote = repo.find_remote(remote_name)?;

        let mut cb = RemoteCallbacks::new();
        cb.credentials(|_url, username, _allowed| {
            if let Some(username) = username {
                Cred::ssh_key_from_agent(username)
            } else {
                Err(git2::Error::from_str("No username provided"))
            }
        });

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(cb);

        remote.fetch(
            &["refs/heads/*:refs/remotes/origin/*"],
            Some(&mut fetch_options),
            None,
        )?;

        let remote_head_ref = format!("refs/remotes/{remote_name}/HEAD");
        let remote_head_ref = match repo.find_reference(&remote_head_ref) {
            Ok(r) => r.resolve()?,
            Err(_) => {
                let err_msg = format!("Remote '{remote_name}' does not have HEAD");
                return Err(anyhow::anyhow!(err_msg));
            }
        };

        let remote_head_oid = match remote_head_ref.target() {
            Some(oid) => oid,
            None => return Err(anyhow::anyhow!("Remote HEAD has no target")),
        };

        let remote_head_commit = repo.find_commit(remote_head_oid)?;
        Ok(remote_head_commit.id().to_string())
    }
}
