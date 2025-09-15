# Command Reference

Global options

- `-v, --verbose` Increase verbosity. Default is info; `-vv` enables debug.
- `-V, --version` Print version.
- `-h, --help` Print help.

## init

- Initialize `pez.toml` under the configuration directory. Fails if it already exists.

## install

- Install from CLI targets or from `pez.toml` (when no targets are given).
- Targets: `owner/repo[@ref]`, `host/owner/repo[@ref]`, full URL, absolute/`~/` paths.
- Options: `--force`, `--prune` (remove lockfile entries not present in `pez.toml`).
- Behavior:
  - CLI‑specified targets are appended to `pez.toml` (relative paths are normalized to absolute).
  - `owner/repo` resolves to `https://github.com/owner/repo`; `host/...` without a scheme is normalized to `https://host/...`.
  - Selectors: `@latest`, `@version:<v>`, `@branch:<b>`, `@tag:<t>`, `@commit:<sha>` influence the resolved commit for fresh installs and `install --force`.
  - Duplicate files: pez maintains a set of destination paths encountered during the run and skips a plugin if copying would overwrite an existing file (applies to both CLI targets and `pez.toml`). A warning is printed and the plugin’s files are not recorded.
  - Concurrency: with explicit targets, clones run concurrently and file copies run sequentially with duplicate‑path detection; installs from `pez.toml` are processed sequentially with the same duplicate detection.

## uninstall

- Remove the specified plugins (`owner/repo`). At least one plugin must be provided.
- Options: `--force` (remove destination files even if the repo directory is missing).
- Behavior: removes the cloned repo (if present) and files recorded in `pez-lock.toml`. Without `--force` and a missing repo directory, the command lists the files and aborts.

## upgrade

- Upgrades specified plugins (`owner/repo ...`), or with no arguments, upgrades plugins listed in `pez.toml`.
- For non‑local plugins, update to the latest commit on the remote's default branch (remote HEAD). Local sources are skipped.
- Selectors in `pez.toml` (version/branch/tag/commit) are not applied by `upgrade`; they affect initial install and re‑install with `install --force`.
- Concurrency is controlled by `PEZ_JOBS`.

## list

- Show installed plugins from `pez-lock.toml`.
- Options: `--format [plain|table|json]`, `--outdated`.
- JSON fields:
  - `list`: `name`, `repo`, `source`, `commit`
  - `list --outdated`: `name`, `repo`, `source`, `current`, `latest`

## prune

- Remove plugins that exist only in the lockfile (i.e., not listed in `pez.toml`).
- Options: `--dry-run`, `--yes`, `--force` (remove destination files even if the repo dir is missing).
- Behavior: if `pez.toml` is empty, the command warns and asks for confirmation unless `--yes` is provided.

## doctor

- Checks the configuration file, lockfile, data/config directories, and the set of copied files.
- Reported checks include: `config`, `lock_file`, `fish_config_dir`, `pez_data_dir`, `repos` (missing clones), `target_files` (missing files), `duplicates` (conflicting destinations).
- Options: `--format json`.

## completions

- Generate completion script for Fish: `pez completions fish > ~/.config/fish/completions/pez.fish`

## migrate

- Import from fisher’s `fish_plugins` into `pez.toml`.
- Options: `--dry-run`, `--force`, `--install`.
