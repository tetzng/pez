use git2::{Cred, Error, FetchOptions, RemoteCallbacks};
use std::path;

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
    // Download all tags to support @tag checkouts.
    fetch_options.download_tags(git2::AutotagOption::All);
    fetch_options
}

pub(crate) fn get_latest_commit_sha(repo: git2::Repository) -> Result<String, git2::Error> {
    let commit = repo.head()?.peel_to_commit()?;

    Ok(commit.id().to_string())
}

/// Attempts to checkout the provided git `refspec` (tag/branch/commit) and returns the checked out commit sha.
#[allow(dead_code)]
pub(crate) fn checkout_ref(repo: &git2::Repository, refspec: &str) -> anyhow::Result<String> {
    // Try to resolve as any object (commit, tag, branch)
    let obj = repo
        .revparse_single(refspec)
        .or_else(|_| repo.revparse_single(&format!("refs/tags/{refspec}")))?;
    repo.set_head_detached(obj.id())?;
    Ok(obj.id().to_string())
}

/// Rough heuristic: a source is a local path if it starts with '/', './', '../', or '~'.
pub(crate) fn is_local_source(source: &str) -> bool {
    source.starts_with('/')
        || source.starts_with("./")
        || source.starts_with("../")
        || source.starts_with('~')
}

pub(crate) fn fetch_all(repo: &git2::Repository) -> anyhow::Result<()> {
    let mut cb = RemoteCallbacks::new();
    cb.credentials(|_url, username, _allowed| {
        if let Some(username) = username {
            Cred::ssh_key_from_agent(username)
        } else {
            Err(git2::Error::from_str("No username provided"))
        }
    });
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(cb);
    fo.download_tags(git2::AutotagOption::All);
    let mut remote = repo.find_remote("origin")?;
    remote.fetch(
        &[
            "refs/heads/*:refs/remotes/origin/*",
            "refs/tags/*:refs/tags/*",
        ],
        Some(&mut fo),
        None,
    )?;
    Ok(())
}

pub(crate) fn get_remote_head_commit(repo: &git2::Repository) -> anyhow::Result<String> {
    fetch_all(repo)?;
    if let Ok(remote) = repo.find_remote("origin")
        && let Ok(buf) = remote.default_branch()
        && let Some(name) = buf.as_str()
        && let Some(branch) = name.strip_prefix("refs/heads/")
        && let Some(oid) = get_remote_branch_commit(repo, branch)?
    {
        return Ok(oid);
    }
    let remote_head_ref = "refs/remotes/origin/HEAD";
    let r = repo.find_reference(remote_head_ref)?.resolve()?;
    let oid = r
        .target()
        .ok_or_else(|| anyhow::anyhow!("Remote HEAD has no target"))?;
    Ok(oid.to_string())
}

pub(crate) fn get_remote_branch_commit(
    repo: &git2::Repository,
    branch: &str,
) -> anyhow::Result<Option<String>> {
    fetch_all(repo)?;
    let refname = format!("refs/remotes/origin/{branch}");
    match repo.find_reference(&refname) {
        Ok(r) => Ok(r.target().map(|oid| oid.to_string())),
        Err(_) => Ok(None),
    }
}

pub(crate) fn get_tag_commit(repo: &git2::Repository, tag: &str) -> anyhow::Result<Option<String>> {
    fetch_all(repo)?;
    let name = format!("refs/tags/{tag}");
    match repo.revparse_single(&name) {
        Ok(obj) => Ok(Some(obj.peel_to_commit()?.id().to_string())),
        Err(_) => Ok(None),
    }
}

pub(crate) fn list_tags(repo: &git2::Repository) -> anyhow::Result<Vec<String>> {
    fetch_all(repo)?;
    let names = repo.tag_names(None)?;
    let mut tags = Vec::new();
    for i in 0..names.len() {
        if let Some(name) = names.get(i) {
            tags.push(name.to_string());
        }
    }
    Ok(tags)
}

pub(crate) enum Selection {
    DefaultHead,
    Latest,
    Branch(String),
    Tag(String),
    Commit(String),
    Version(String),
}

pub(crate) fn resolve_selection(
    repo: &git2::Repository,
    sel: &Selection,
) -> anyhow::Result<String> {
    match sel {
        Selection::DefaultHead | Selection::Latest => get_remote_head_commit(repo),
        Selection::Branch(name) => {
            if let Some(c) = get_remote_branch_commit(repo, name)? {
                tracing::debug!(branch = name, commit = %c, "Resolved branch to commit");
                Ok(c)
            } else {
                anyhow::bail!(format!("Branch not found: {name}"))
            }
        }
        Selection::Tag(t) => {
            if let Some(c) = get_tag_commit(repo, t)? {
                tracing::debug!(tag = t, commit = %c, "Resolved tag to commit");
                Ok(c)
            } else {
                anyhow::bail!(format!("Tag not found: {t}"))
            }
        }
        Selection::Commit(sha) => {
            let obj = repo
                .revparse_single(sha)
                .map_err(|e| anyhow::anyhow!("Failed to resolve commit '{sha}': {e}"))?;
            let id = obj.peel_to_commit()?.id().to_string();
            tracing::debug!(commit = %id, "Resolved explicit commit");
            Ok(id)
        }
        Selection::Version(v) => {
            let id = resolve_version(repo, v)?;
            tracing::debug!(version = v, commit = %id, "Resolved version to commit");
            Ok(id)
        }
    }
}

fn resolve_version(repo: &git2::Repository, v: &str) -> anyhow::Result<String> {
    if v == "latest" {
        return get_remote_head_commit(repo);
    }
    if let Some(c) = get_remote_branch_commit(repo, v)? {
        return Ok(c);
    }
    let tags = list_tags(repo)?;
    if let Some(tag) = pick_tag_for_version(&tags, v)?
        && let Some(c) = get_tag_commit(repo, &tag)?
    {
        return Ok(c);
    }
    anyhow::bail!(format!("No matching branch or tag for version: {v}"))
}

fn pick_tag_for_version(tags: &[String], v: &str) -> anyhow::Result<Option<String>> {
    use semver::Version;
    let v_trim = v.trim_start_matches('v');
    let parts: Vec<&str> = v_trim.split('.').collect();
    let mut semver_tags: Vec<(Version, String)> = Vec::new();
    for t in tags {
        let name = t.trim();
        let name_trim = name.trim_start_matches('v');
        if let Ok(ver) = Version::parse(name_trim) {
            // Exclude pre-release tags by default
            if ver.pre.is_empty() {
                semver_tags.push((ver, name.to_string()));
            }
        }
    }
    if !semver_tags.is_empty() {
        if parts.len() == 3
            && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
            && let Ok(want) = Version::parse(v_trim)
            && let Some((_, tag)) = semver_tags.iter().find(|(sv, _)| *sv == want)
        {
            tracing::debug!(version = %v, tag = %tag, "Matched exact semver tag");
            return Ok(Some(tag.clone()));
        }
        let want_major = parts.first().and_then(|s| s.parse::<u64>().ok());
        let want_minor = parts.get(1).and_then(|s| s.parse::<u64>().ok());
        if let Some(mj) = want_major {
            let mut candidates: Vec<(Version, String)> = semver_tags
                .into_iter()
                .filter(|(sv, _)| sv.major == mj && want_minor.is_none_or(|mn| sv.minor == mn))
                .collect();
            if !candidates.is_empty() {
                candidates.sort_by(|a, b| a.0.cmp(&b.0));
                let tag = candidates.last().map(|(_, tag)| tag.clone());
                if let Some(ref t) = tag {
                    tracing::debug!(version = %v, tag = %t, "Selected highest semver tag by prefix");
                }
                return Ok(tag);
            }
        }
    }
    if tags.iter().any(|t| t == v) {
        tracing::debug!(version = %v, tag = %v, "Matched non-semver exact tag");
        return Ok(Some(v.to_string()));
    }
    let mut candidates: Vec<(Vec<u64>, String)> = Vec::new();
    for t in tags {
        if t == v {
            return Ok(Some(t.clone()));
        }
        if let Some(rest) = t.strip_prefix(&format!("{v}.")) {
            let nums: Vec<u64> = rest
                .split('.')
                .map(|s| s.parse::<u64>().unwrap_or(0))
                .collect();
            candidates.push((nums, t.clone()));
        } else if let Some(rest) = t.strip_prefix(&format!("v{v}.")) {
            let nums: Vec<u64> = rest
                .split('.')
                .map(|s| s.parse::<u64>().unwrap_or(0))
                .collect();
            candidates.push((nums, t.clone()));
        }
    }
    if !candidates.is_empty() {
        candidates.sort_by(|a, b| a.0.cmp(&b.0));
        let tag = candidates.last().map(|(_, tag)| tag.clone());
        if let Some(ref t) = tag {
            tracing::debug!(version = %v, tag = %t, "Selected highest non-semver dotted suffix tag");
        }
        return Ok(tag);
    }
    Ok(None)
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
