# Configuration and Lockfile

This document describes the user‑facing configuration files used by pez.

## pez.toml

Define the plugins you want pez to manage. Each entry must specify exactly one
source kind and at most one version selector.

Rules

- Source: choose exactly one of `repo` (GitHub shorthand), `url` (full Git URL), or `path` (local directory).
- Selector: choose at most one of `version`, `branch`, `tag`, or `commit`.

GitHub shorthand (repo source)

```toml
[[plugins]]
repo = "owner/repo"
# version = "latest"   # default if omitted; or "v3" (branch preferred over tags)
# branch  = "main"
# tag     = "v1.2.3"
# commit  = "<sha>"    # 7+ chars recommended
```

Generic Git host (url source)

```toml
[[plugins]]
url = "https://gitlab.com/owner/repo"
# version = "v3"
# branch  = "main"
# tag     = "v1.2.3"
# commit  = "<sha>"
```

Local directory (path source)

```toml
[[plugins]]
path = "~/path/to/local/plugin"   # absolute or ~/ only
```

Notes

- If a URL has no scheme, pez normalizes it to https (e.g., `gitlab.com/...`).
- CLI‑provided relative paths are normalized to absolute paths when recorded.
- `path` must resolve to an absolute path (either absolute or `~/…`).

## pez-lock.toml

Machine‑generated; do not edit. The lock file records the concrete state pez has
installed: `name`, `repo`, `source`, `commit_sha`, and copied `files`.

Example

```toml
version = 1

[[plugins]]
name = "repo"
repo = "owner/repo"
source = "https://github.com/owner/repo"
commit_sha = "abc1234..."

  [[plugins.files]]
  dir = "functions"
  name = "foo.fish"

  [[plugins.files]]
  dir = "conf.d"
  name = "bar.fish"
```

Notes

- For local sources, `commit_sha = "local"`. Such entries are skipped by
  `upgrade` and excluded from `list --outdated` comparisons.

## Environment Variables

- `PEZ_CONFIG_DIR` — Directory containing `pez.toml` and `pez-lock.toml`.
- `PEZ_DATA_DIR` — Base directory for cloned plugin repositories.
- `PEZ_TARGET_DIR` — Override destination Fish config directory used for copying plugin files.
- `__fish_config_dir` / `XDG_CONFIG_HOME` — Fish configuration directory.
- `__fish_user_data_dir` / `XDG_DATA_HOME` — Fish data directory.
- `PEZ_JOBS` — Concurrency for `upgrade`, `uninstall`, and `prune` (default: 4).
- `RUST_LOG` — Log filtering (takes precedence over `-v`).
