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
#
# Non-GitHub host example
# [[plugins]]
# repo = "gitlab.com/owner/repo"
# version = "latest"
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
- Host-prefixed repos (e.g., `gitlab.com/owner/repo`) are recorded as-is and cloned under `<host>/<owner>/<repo>` inside the data directory. GitHub shorthand (`owner/repo`) continues to map to `github.com`.

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
- `PEZ_TARGET_DIR` — Override the Fish config directory used for copying plugin files. It no longer changes where `pez.toml` or `pez-lock.toml` live.
- `__fish_config_dir` / `XDG_CONFIG_HOME` — Fish configuration directory.
- `__fish_user_data_dir` / `XDG_DATA_HOME` — Fish data directory.
- `--jobs <N>` — Global CLI flag to override concurrency for `install` (explicit
  targets), `upgrade`, `uninstall`, and `prune`. Must be a positive integer.
- `PEZ_JOBS` — Environment override for the same concurrency (default: 4). Ignored
  when `--jobs` is provided.
- `RUST_LOG` — Log filtering (takes precedence over `-v`).

### Migration Note (PEZ_TARGET_DIR)

Releases after September 16, 2025 keep configuration files under the config
precedence `PEZ_CONFIG_DIR` → `__fish_config_dir` → `XDG_CONFIG_HOME/fish` →
`~/.config/fish`, even when `PEZ_TARGET_DIR` is set. If your existing
`pez.toml` or `pez-lock.toml` live in a path referenced solely by
`PEZ_TARGET_DIR`, move them into a directory referenced by `PEZ_CONFIG_DIR`
or set `PEZ_CONFIG_DIR` to that path before invoking pez.
