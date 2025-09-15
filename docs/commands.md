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
  - Duplicate files: pez tracks destination paths seen during the run and skips a plugin if copying would overwrite an existing file (applies to both CLI targets and `pez.toml`). A warning is printed and the plugin’s files are not recorded.
  - Concurrency: with explicit targets, clones run concurrently (bounded by `PEZ_JOBS`) and file copies run sequentially with duplicate‑path detection; installs from `pez.toml` are processed sequentially with the same duplicate detection.
  - Existing clones: when the repository directory already exists, CLI targets are skipped with a warning unless `--force` is provided; installs from `pez.toml` return an error for that plugin unless `--force` is provided.

## uninstall

- Remove the specified plugins (`owner/repo`). With `--stdin`, also read plugin repos from standard input (one per line).
- Options:
  - `--force` Remove files recorded in the lockfile even if the repository directory is missing.
  - `--stdin` Read `owner/repo` list from stdin (ignores blank lines and lines starting with `#`).
- Behavior: removes the cloned repository (if present) and files recorded in `pez-lock.toml`. Without `--force` when the repo directory is missing, the command prints the target files and exits.
- Example:
  - `printf "owner/a\nowner/b\n" | pez uninstall --stdin`

## upgrade

- Upgrade specified plugins (`owner/repo ...`), or with no arguments, upgrade plugins listed in `pez.toml`.
- Respects selectors in `pez.toml` (`version`/`branch`/`tag`/`commit`). When no selector is set, updates to the latest commit on the remote default branch (remote HEAD).
- Local path sources (`path`) are skipped.
- Concurrency is controlled by `PEZ_JOBS`.

## list

- Show installed plugins recorded in `pez-lock.toml`.
- Options:
  - `--format [plain|table|json]`
  - `--outdated`
  - `--filter [all|local|remote]`
- Fields:
  - table: `name`, `repo`, `source`, `selector`, `commit`
  - json: `name`, `repo`, `source`, `selector`, `commit`
  - `list --outdated` (json/table): `name`, `repo`, `source`, `current`, `latest`

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
