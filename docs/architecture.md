# Architecture

This document outlines the high‑level structure and flows in pez.

## Overview

- `main.rs` initializes logging and dispatches to subcommands defined in `cli.rs` and implemented under `cmd/*`.
- Core modules:
  - `models.rs`: shared domain types (PluginRepo, InstallTarget, ResolvedInstallTarget, TargetDir).
  - `config.rs`: load/save `pez.toml`, convert entries to install targets.
  - `lock_file.rs`: load/save `pez-lock.toml`, track installed plugins and copied files.
  - `resolver.rs`: parse refs (latest/version/tag/branch/commit) and map to `Selection`.
  - `git.rs`: resolve selections against a repo (branches/tags/commits), list tags.
  - `utils.rs`: path/env resolution, copy routines, events, helpers.
  - `cmd/*`: end‑user commands orchestrating core modules.
    - `cmd/activate.rs`: emits Fish wrapper code to run hooks in-shell.
    - `cmd/files.rs`: lists installed file paths from the lockfile (used by activation).

## Data Flow (install)

1. Normalize CLI targets (or entries in `pez.toml`) into `InstallTarget` values.
2. Convert each `InstallTarget` to a `ResolvedInstallTarget` (source, ref_kind, is_local).
3. Clone remote sources; skip clone for local paths.
4. Resolve the commit using `resolver::RefKind` -> `git::resolve_selection`.
5. Copy files to the Fish config directory using `utils::copy_plugin_files*`.
6. Update the lockfile with `name`/`repo`/`source`/`commit_sha`/`files`.
7. For files under `conf.d`, emit `fish -c 'emit <stem>_{install|update|uninstall}'` events.

## Concurrency

- `--jobs <N>` globally overrides concurrency for `upgrade`, `uninstall`, `prune`,
  and the clone phase of `install` when explicit targets are provided. When the
  flag is absent, `PEZ_JOBS` acts as the environment override (default: 4).
- `install` concurrency depends on how it is invoked:
  - With explicit targets (`install <targets...>`): clones run concurrently
    (bounded by the configured job limit), file copies run sequentially with
    duplicate‑path detection and warnings.
  - From `pez.toml` (no targets): processing is sequential and uses the same
    duplicate‑path detection; conflicting plugins are skipped with a warning.

## Paths and Resolution

- Config dir precedence: `PEZ_CONFIG_DIR` > `__fish_config_dir` > `XDG_CONFIG_HOME/fish` > `~/.config/fish`.
- `PEZ_TARGET_DIR` only adjusts the copy destination; configuration and lock files stay under the config precedence above.
- Data dir precedence: `PEZ_DATA_DIR` > `__fish_user_data_dir/pez` > `XDG_DATA_HOME/fish/pez` > `~/.local/share/fish/pez`.
- Copy destination: defaults to the Fish config directory and can be overridden via `PEZ_TARGET_DIR` (or falls back to `__fish_config_dir` / `XDG_CONFIG_HOME/fish`).

## Upgrade Semantics

- Local sources are skipped.
- If a selector is specified in `pez.toml` (`version`/`branch`/`tag`/`commit`), `upgrade` resolves against that selector.
- When no selector is set, non‑local plugins update to the latest commit on the remote default branch (remote HEAD).
