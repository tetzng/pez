# Configuration and Lockfile

This document describes the user‑facing configuration files used by pez.

## Locations and Precedence

- Config files (`pez.toml`, `pez-lock.toml`):
  `PEZ_CONFIG_DIR` > `__fish_config_dir` > `XDG_CONFIG_HOME/fish` > `~/.config/fish`
- Data directory (cloned repos):
  `PEZ_DATA_DIR` > `__fish_user_data_dir/pez` > `XDG_DATA_HOME/fish/pez` > `~/.local/share/fish/pez`
- Copy destination:
  `PEZ_TARGET_DIR` > `__fish_config_dir` > `XDG_CONFIG_HOME/fish` > `~/.config/fish`

`PEZ_TARGET_DIR` only affects where plugin files are copied; configuration and
lock files always live under the config precedence above.

## pez.toml

Define the plugins you want pez to manage. Each entry must specify exactly one
source kind and at most one version selector.

Rules

- Source: choose exactly one of `repo` (GitHub shorthand), `url` (full Git URL), or `path` (local directory).
- Selector: choose at most one of `version`, `branch`, `tag`, or `commit`.
- Name (optional): set `name = "..."` to override the display name recorded in the lockfile and shown in `list`.

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
- CLI‑provided relative paths and `~/` are normalized to absolute paths when recorded.
- `path` must resolve to an absolute path (either absolute or `~/…`).
- Host-prefixed repos (e.g., `gitlab.com/owner/repo`) are recorded as-is and cloned under `<host>/<owner>/<repo>` inside the data directory. GitHub shorthand (`owner/repo`) continues to map to `github.com`.
- Unknown keys in `pez.toml` are rejected at load time.
- `path` sources cannot include version selectors (`version`/`branch`/`tag`/`commit`).

## JSON Schema

`config.schema.json` provides a JSON Schema representation of the `pez.toml`
plugin spec rules.

Regenerate it with:

```sh
cargo run --features schema-gen --bin gen-config-schema
```

When changing config-related types or validation rules, regenerate
`config.schema.json` and include the updated file in the same commit.

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

## Plugin Layout and Copy Rules

- pez looks for top-level `functions`, `completions`, `conf.d`, and `themes` directories in each plugin repo.
- It copies files recursively into the matching Fish config directories, preserving relative paths.
- Only `.fish` files are copied from `functions`/`completions`/`conf.d`, and only `.theme` files from `themes`.
- If two plugins would write the same destination path in a single run, the later plugin is skipped and its files are not recorded in the lockfile.
- For `conf.d` files, pez emits `emit <stem>_{install|update|uninstall}` after installs/upgrades or before uninstalls (unless `PEZ_SUPPRESS_EMIT` is set).

## Environment Variables and CLI Overrides

- `PEZ_CONFIG_DIR` — Directory containing `pez.toml` and `pez-lock.toml`.
- `PEZ_DATA_DIR` — Base directory for cloned plugin repositories.
- `PEZ_TARGET_DIR` — Override the Fish config directory used for copying plugin files. It no longer changes where `pez.toml` or `pez-lock.toml` live.
- `PEZ_SUPPRESS_EMIT` — When set, suppress `fish -c 'emit ...'` hooks during install/upgrade/uninstall. Used by `pez activate fish` to avoid duplicate events.
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
