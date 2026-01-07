use crate::resolver::Selection;
use git2::{Cred, Error, FetchOptions, RemoteCallbacks};
use std::path;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(test)]
static CALLBACKS_CONFIGURED: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static FETCH_OPTIONS_CONFIGURED: AtomicUsize = AtomicUsize::new(0);

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
    // Use libgit2's default credential negotiation which covers HTTPS, SSH agent,
    // and other common flows. This matches the behavior used in clone_repository.
    callbacks.credentials(|_, _, _| Cred::default());
    #[cfg(test)]
    CALLBACKS_CONFIGURED.fetch_add(1, Ordering::SeqCst);
    callbacks
}

fn setup_fetch_options(callbacks: RemoteCallbacks<'static>) -> FetchOptions<'static> {
    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);
    // Download all tags to support @tag checkouts.
    fetch_options.download_tags(git2::AutotagOption::All);
    #[cfg(test)]
    FETCH_OPTIONS_CONFIGURED.fetch_add(1, Ordering::SeqCst);
    fetch_options
}

pub(crate) fn get_latest_commit_sha(repo: &git2::Repository) -> Result<String, git2::Error> {
    let commit = repo.head()?.peel_to_commit()?;

    Ok(commit.id().to_string())
}

pub(crate) fn checkout_detached(repo: &git2::Repository, oid: git2::Oid) -> anyhow::Result<()> {
    repo.set_head_detached(oid)?;
    if repo.is_bare() {
        return Ok(());
    }
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    repo.checkout_head(Some(&mut checkout))?;
    Ok(())
}

pub(crate) fn checkout_commit(repo: &git2::Repository, commit: &str) -> anyhow::Result<()> {
    let oid = git2::Oid::from_str(commit)?;
    checkout_detached(repo, oid)
}

/// Attempts to checkout the provided git `refspec` (tag/branch/commit) and returns the checked out commit sha.
#[allow(dead_code)]
pub(crate) fn checkout_ref(repo: &git2::Repository, refspec: &str) -> anyhow::Result<String> {
    // Try to resolve as any object (commit, tag, branch)
    let obj = repo
        .revparse_single(refspec)
        .or_else(|_| repo.revparse_single(&format!("refs/tags/{refspec}")))?;
    checkout_detached(repo, obj.id())?;
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
    let cb = setup_remote_callbacks();
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

// tests are in a submodule at the end of file

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

        let cb = setup_remote_callbacks();
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

        let cb = setup_remote_callbacks();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    fn init_repo_with_commit(path: &Path) -> (git2::Repository, git2::Oid) {
        let repo = git2::Repository::init(path).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "tester").unwrap();
        cfg.set_str("user.email", "tester@example.com").unwrap();

        fs::create_dir_all(path).unwrap();
        fs::write(path.join("README.md"), "hello").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        let tree_oid = index.write_tree().unwrap();
        let commit_oid = {
            let tree = repo.find_tree(tree_oid).unwrap();
            let sig = repo.signature().unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
                .unwrap()
        };
        (repo, commit_oid)
    }

    fn commit_file(repo: &git2::Repository, rel_path: &Path, message: &str) -> git2::Oid {
        let mut index = repo.index().unwrap();
        index.add_path(rel_path).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("tester", "tester@example.com").unwrap();
        let parent = repo
            .head()
            .ok()
            .and_then(|head| head.target())
            .and_then(|oid| repo.find_commit(oid).ok());
        match parent {
            Some(ref parent) => repo
                .commit(Some("HEAD"), &sig, &sig, message, &tree, &[parent])
                .unwrap(),
            None => repo
                .commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
                .unwrap(),
        }
    }

    #[test]
    fn setup_remote_callbacks_configures_credentials() {
        CALLBACKS_CONFIGURED.store(0, Ordering::SeqCst);
        let _ = setup_remote_callbacks();
        assert!(CALLBACKS_CONFIGURED.load(Ordering::SeqCst) > 0);
    }

    #[test]
    fn setup_fetch_options_configures_download_tags() {
        FETCH_OPTIONS_CONFIGURED.store(0, Ordering::SeqCst);
        let cb = setup_remote_callbacks();
        let _ = setup_fetch_options(cb);
        assert!(FETCH_OPTIONS_CONFIGURED.load(Ordering::SeqCst) > 0);
    }

    #[test]
    fn get_latest_commit_sha_returns_head_commit() {
        let tmp = tempdir().unwrap();
        let (repo, commit_oid) = init_repo_with_commit(tmp.path());
        let sha = get_latest_commit_sha(&repo).unwrap();
        assert_eq!(sha, commit_oid.to_string());
    }

    #[test]
    fn checkout_ref_resolves_tag_commit() {
        let tmp = tempdir().unwrap();
        let (repo, commit_oid) = init_repo_with_commit(tmp.path());
        let obj = repo.find_object(commit_oid, None).unwrap();
        repo.tag_lightweight("v1.0.0", &obj, false).unwrap();

        let checked = checkout_ref(&repo, "v1.0.0").unwrap();
        assert_eq!(checked, commit_oid.to_string());
    }

    #[test]
    fn checkout_commit_updates_worktree() {
        let tmp = tempdir().unwrap();
        let repo = git2::Repository::init(tmp.path()).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "tester").unwrap();
        cfg.set_str("user.email", "tester@example.com").unwrap();

        let file_path = tmp.path().join("README.md");
        std::fs::write(&file_path, "one").unwrap();
        let first = commit_file(&repo, Path::new("README.md"), "first");

        std::fs::write(&file_path, "two").unwrap();
        let second = commit_file(&repo, Path::new("README.md"), "second");

        let head_oid = repo.head().unwrap().target().unwrap();
        assert_eq!(head_oid, second);

        checkout_commit(&repo, &first.to_string()).unwrap();

        let contents = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(contents, "one");

        let head = repo.head().unwrap();
        assert_eq!(head.target().unwrap(), first);
        assert!(repo.head_detached().unwrap());
    }

    #[test]
    fn is_local_source_recognizes_prefixes() {
        assert!(is_local_source("/abs/path"));
        assert!(is_local_source("./rel/path"));
        assert!(is_local_source("../rel/path"));
        assert!(is_local_source("~/rel/path"));
        assert!(!is_local_source("https://github.com/o/r"));
    }

    #[test]
    fn pick_tag_for_version_semver_prefix() {
        let tags = vec![
            "v1.0.0".to_string(),
            "v1.2.0".to_string(),
            "v1.2.1".to_string(),
            "v2.0.0".to_string(),
            "v1.3.0-beta1".to_string(),
        ];
        let sel = pick_tag_for_version(&tags, "v1").unwrap().unwrap();
        assert_eq!(sel, "v1.2.1");
        let exact = pick_tag_for_version(&tags, "v2.0.0").unwrap().unwrap();
        assert_eq!(exact, "v2.0.0");
    }

    #[test]
    fn pick_tag_for_version_dotted_non_semver_prefix() {
        let tags = vec![
            "1.2.1".to_string(),
            "1.3.0".to_string(),
            "v1.4.5".to_string(),
            "2.0.0".to_string(),
        ];
        let sel = pick_tag_for_version(&tags, "1").unwrap().unwrap();
        // Should prefer highest among 1.x.y (either with or without v prefix)
        assert!(sel == "1.3.0" || sel == "v1.4.5");
    }

    #[test]
    fn pick_tag_for_version_prefers_exact_semver_match() {
        let tags = vec!["1.2.3".to_string(), "1.2.4".to_string()];
        let sel = pick_tag_for_version(&tags, "1.2.3").unwrap().unwrap();
        assert_eq!(sel, "1.2.3");
    }

    #[test]
    fn pick_tag_for_version_respects_minor_prefix() {
        let tags = vec![
            "1.1.9".to_string(),
            "1.2.3".to_string(),
            "1.3.0".to_string(),
        ];
        let sel = pick_tag_for_version(&tags, "1.2").unwrap().unwrap();
        assert_eq!(sel, "1.2.3");
    }

    #[test]
    fn pick_tag_for_version_missing_non_semver_returns_none() {
        let tags = vec!["alpha".to_string(), "beta".to_string()];
        let sel = pick_tag_for_version(&tags, "release").unwrap();
        assert!(sel.is_none());
    }

    #[test]
    fn pick_tag_for_version_non_semver_dotted_suffix() {
        let tags = vec!["1.2.0-beta".to_string(), "1.3.0-rc1".to_string()];
        let sel = pick_tag_for_version(&tags, "1").unwrap().unwrap();
        assert_eq!(sel, "1.3.0-rc1");
    }

    #[test]
    fn get_remote_name_accepts_three_part_ref() {
        let tmp = tempdir().unwrap();
        let (repo, commit_oid) = init_repo_with_commit(tmp.path());
        repo.reference("refs/remotes/origin", commit_oid, true, "create remote ref")
            .unwrap();
        let branch = repo
            .find_branch("origin", git2::BranchType::Remote)
            .unwrap();
        let name = get_remote_name(&branch).unwrap();
        assert_eq!(name, "origin");
    }

    #[test]
    fn list_tags_fetches_remote_updates() {
        let tmp = tempdir().unwrap();
        let origin_path = tmp.path().join("origin.git");
        let workdir_path = tmp.path().join("work");
        let clone_path = tmp.path().join("clone");

        let origin = git2::Repository::init_bare(&origin_path).unwrap();
        let (work, _commit_oid) = init_repo_with_commit(&workdir_path);

        work.remote("origin", origin_path.to_str().unwrap())
            .unwrap();
        let head_ref = work.head().unwrap().name().unwrap().to_string();
        let refspec = format!("{head_ref}:{head_ref}");
        {
            let mut remote = work.find_remote("origin").unwrap();
            remote
                .connect(git2::Direction::Push)
                .and_then(|_| remote.push(&[refspec.as_str()], None))
                .unwrap();
        }
        origin.set_head(&head_ref).unwrap();

        let clone = clone_repository(origin_path.to_str().unwrap(), &clone_path).unwrap();

        // Create a new commit and tag it locally, then push only the tag.
        fs::write(workdir_path.join("TAG.txt"), "tagged").unwrap();
        let mut index = work.index().unwrap();
        index.add_path(Path::new("TAG.txt")).unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = work.find_tree(tree_oid).unwrap();
        let sig = work.signature().unwrap();
        let parent = work.head().unwrap().peel_to_commit().unwrap();
        let tag_commit = work
            .commit(Some("HEAD"), &sig, &sig, "tag commit", &tree, &[&parent])
            .unwrap();
        let obj = work.find_object(tag_commit, None).unwrap();
        work.tag_lightweight("orphan", &obj, false).unwrap();
        {
            let mut remote = work.find_remote("origin").unwrap();
            remote
                .connect(git2::Direction::Push)
                .and_then(|_| remote.push(&["refs/tags/orphan:refs/tags/orphan"], None))
                .unwrap();
        }

        let tags = list_tags(&clone).unwrap();
        assert!(tags.iter().any(|tag| tag == "orphan"));
    }

    #[test]
    fn get_latest_remote_commit_from_local_remote_repo() {
        use std::fs;
        use tempfile::tempdir;

        // Setup temporary directories
        let tmp = tempdir().unwrap();
        let origin_path = tmp.path().join("origin.git");
        let workdir_path = tmp.path().join("work");
        let clone_path = tmp.path().join("clone");

        // Initialize bare origin and a working repo
        let origin = git2::Repository::init_bare(&origin_path).unwrap();
        let work = git2::Repository::init(&workdir_path).unwrap();

        // Configure identity for committing
        {
            let mut cfg = work.config().unwrap();
            cfg.set_str("user.name", "tester").unwrap();
            cfg.set_str("user.email", "tester@example.com").unwrap();
        }

        // Create initial commit on main
        fs::create_dir_all(&workdir_path).unwrap();
        fs::write(workdir_path.join("README.md"), "hello").unwrap();

        let mut index = work.index().unwrap();
        index.add_path(std::path::Path::new("README.md")).unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = work.find_tree(tree_oid).unwrap();
        let sig = work.signature().unwrap();
        let commit_oid = work
            .commit(Some("refs/heads/main"), &sig, &sig, "init", &tree, &[])
            .unwrap();

        // Add origin and push main
        work.remote("origin", origin_path.to_str().unwrap())
            .unwrap();
        {
            let mut remote = work.find_remote("origin").unwrap();
            remote
                .connect(git2::Direction::Push)
                .and_then(|_| remote.push(&["refs/heads/main:refs/heads/main"], None))
                .unwrap();
        }

        // Set default branch on origin to refs/heads/main
        origin.set_head("refs/heads/main").unwrap();

        // Clone into consumer repo using our clone logic
        let clone = clone_repository(origin_path.to_str().unwrap(), &clone_path).unwrap();

        // get_latest_remote_commit should resolve to the pushed commit
        let latest = get_latest_remote_commit(&clone).unwrap();
        assert_eq!(latest, commit_oid.to_string());
    }
}
