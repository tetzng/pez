# Command Reference

## Contents

- [Usage](#usage)
- [Commands](#commands)
  - [init](#init)
  - [install](#install)
  - [uninstall](#uninstall)
  - [upgrade](#upgrade)
  - [list](#list)
  - [prune](#prune)
  - [doctor](#doctor)
  - [completions](#completions)
  - [activate](#activate)
  - [files](#files)
  - [migrate](#migrate)

## Usage

```text
pez [OPTIONS] <COMMAND>
```

Global options (apply to all commands)

| Option | Description |
| --- | --- |
| `-v, --verbose` | Increase verbosity. Default is info; `-vv` enables debug. |
| `--jobs <N>` | Override parallel job limit for commands that spawn concurrent tasks (defaults to 4; overrides `PEZ_JOBS`). |
| `-V, --version` | Print version. |
| `-h, --help` | Print help. |

## Commands

### init

- Initialize `pez.toml` under the configuration directory. Fails if it already exists.

### install

- Install from CLI targets or from `pez.toml` (when no targets are given).
- Targets: `owner/repo[@ref]`, `host/owner/repo[@ref]`, full URL, local paths (absolute, `~/`, or relative).
- Options:
  - `--force` Reinstall even if the target already exists.
  - `--prune` (only available when running without explicit targets) removes lockfile entries that are no longer declared in `pez.toml` after a successful install.
- Behavior:
  - CLI‑specified targets are appended to `pez.toml`; relative paths and `~/` are normalized to absolute paths before writing.
  - `owner/repo` resolves to `https://github.com/owner/repo`; `host/...` without a scheme is normalized to `https://host/...`.
  - Selectors: `@latest`, `@version:<v>`, `@branch:<b>`, `@tag:<t>`, `@commit:<sha>` influence the resolved commit for fresh installs and `install --force`.
  - `@ref` parsing applies to shorthand/host targets without a scheme; full URLs are treated as literal strings. Use `pez.toml` to pin refs for URL installs.
  - File selection: only `.fish` files are copied from `functions`/`completions`/`conf.d`, and only `.theme` files from `themes`.
  - Duplicate files: pez tracks destination paths seen during the run and skips a plugin if copying would overwrite an existing file (applies to both CLI targets and `pez.toml`). A warning is printed and the plugin’s files are not recorded.
  - Concurrency: with explicit targets, clones run concurrently (bounded by `--jobs` or `PEZ_JOBS`) and file copies run sequentially with duplicate‑path detection; installs from `pez.toml` are processed sequentially with the same duplicate detection.
  - Existing clones: CLI targets are skipped with a warning unless you pass `--force`, which removes the cached clone before re-cloning. When running from `pez.toml`, entries that already exist in `pez-lock.toml` and on disk are treated as up to date and skipped unless you pass `--force`; when `--force` is present, pez deletes the cached clone before re-cloning so config-driven installs behave the same as explicit targets. If a clone exists without a matching lockfile entry, pez returns an error unless you pass `--force`.
  - Clone path layout: remote repos live under `<host>/<owner>/<repo>` in the data directory. GitHub shorthand (`owner/repo`) continues to resolve to `github.com`.
  - With `--prune`, pez removes lockfile entries that are no longer declared in `pez.toml` after a successful install (similar to `pez prune`).

### uninstall

- Remove the specified plugins (`owner/repo` or `host/owner/repo`). With `--stdin`, also read plugin repos from standard input (one per line).
- Options:
  - `--force` Remove files recorded in the lockfile even if the repository directory is missing.
  - `--stdin` Read `owner/repo` or `host/owner/repo` values from stdin. Blank lines and lines starting with `#` are ignored; the remaining entries are sorted and deduplicated before processing.
- Behavior: removes the cloned repository (if present) and the files recorded in `pez-lock.toml`, then removes the matching entry from `pez.toml` to keep the configuration in sync. Without `--force` when the repo directory is missing, the command prints the target files and exits.
- Example:
  - `printf "owner/a\nowner/b\n" | pez uninstall --stdin`

### upgrade

- Upgrade specified plugins (`owner/repo` or `host/owner/repo`), or with no arguments, upgrade plugins listed in `pez.toml`.
- Respects selectors in `pez.toml` (`version`/`branch`/`tag`/`commit`). When no selector is set, updates to the latest commit on the remote default branch (remote HEAD).
- Local path sources (`path`) are skipped.
- Concurrency is controlled by `--jobs` or `PEZ_JOBS`.
- Any repo specified on the CLI that is not already in `pez.toml` is added automatically so future installs remain in sync.

### list

- Show installed plugins recorded in `pez-lock.toml`.
- Options:
  - `--format [plain|table|json]`
  - `--outdated`
  - `--filter [all|local|remote]`
- Filtering is based on the plugin source: `local` shows only path-based installs, `remote` keeps Git-backed sources.
- Fields:
  - table: `name`, `repo`, `source`, `selector`, `commit`
  - json: `name`, `repo`, `source`, `selector`, `commit`
  - `list --outdated` (json/table): `name`, `repo`, `source`, `current`, `latest`

### prune

- Remove plugins that exist only in the lockfile (i.e., not listed in `pez.toml`).
- Options: `--dry-run`, `--yes`, `--force` (remove destination files even if the repo dir is missing).
- Behavior: if `pez.toml` has no `[[plugins]]` entries (plugins list missing), the command warns and asks for confirmation unless `--yes` is provided.

### doctor

- Checks the configuration file, lockfile, data/config directories, and the set of copied files.
- Reported checks include: `config`, `lock_file`, `fish_config_dir`, `pez_data_dir`, `repos` (missing clones), `target_files` (missing files), `duplicates` (conflicting destinations).
- Options: `--format json`.

### completions

- Generate completion script for Fish: `pez completions fish > ~/.config/fish/completions/pez.fish`

### activate

- Output shell activation code that wraps `pez` with hooks in the current shell.
- Usage: `pez activate fish | source` (for persistence, add inside `if status is-interactive ... end` in `~/.config/fish/config.fish`).
- Behavior: after `install`/`upgrade`, sources matching `conf.d` files and emits `<stem>_{install|update}` in the current shell; before `uninstall`, emits `<stem>_uninstall`.
- When active, the wrapper runs `pez` with `PEZ_SUPPRESS_EMIT=1` to avoid duplicate out-of-process emits.

### files

- List installed files recorded in `pez-lock.toml`.
- Plugin identifiers: `owner/repo`, `host/owner/repo`, or URLs; `@ref` suffixes are accepted for shorthand/host forms and ignored for lookup.
- Options:
  - `--all` list files for all installed plugins.
  - `--dir [conf.d|all]` filter destinations.
  - `--format [paths|json]` output format.
  - `--from [install|update|upgrade|uninstall|remove]` derive plugins by parsing a subcommand; pass the subcommand args after `--` (`update`/`remove` are aliases for `upgrade`/`uninstall`).
- Examples:
  - `pez files --all`
  - `pez files owner/repo --dir conf.d`
  - `pez files --from install -- owner/repo@v3`
  - `printf "owner/a\n" | pez files --from uninstall -- --stdin`

### migrate

- Import from fisher’s `fish_plugins` into `pez.toml`.
- By default the command merges new repos into the existing `pez.toml`, skipping duplicates, ignoring comments/blank lines, and omitting the `jorgebucaran/fisher` entry itself.
- Pinned refs such as `owner/repo@2.0.0`, `owner/repo@tag:v1`, or `host/owner/repo@branch:main` are preserved; if an entry was already pinned in `pez.toml`, migrating to a different ref updates it, while unpinned incoming entries leave the existing pin untouched. URL-based entries that append `@ref` as part of the URL or lines with an empty suffix (e.g. `owner/repo@`) are ignored to avoid writing invalid specs—convert them to `owner/repo@ref` form before migrating.
- `--dry-run` prints the planned additions without modifying any files.
- `--force` replaces the existing plugin list with the migrated entries instead of merging.
- `--install` triggers `pez install` for the migrated entries after they are written (skipped when `--dry-run` is set).
