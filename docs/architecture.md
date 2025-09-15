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

## Data Flow (install)

1. Normalize CLI targets (or entries in `pez.toml`) into `InstallTarget` values.
2. Convert each `InstallTarget` to a `ResolvedInstallTarget` (source, ref_kind, is_local).
3. Clone remote sources; skip clone for local paths.
4. Resolve the commit using `resolver::RefKind` → `git::resolve_selection`.
5. Copy files to the Fish config directory using `utils::copy_plugin_files*`.
6. Update the lockfile with `name`/`repo`/`source`/`commit_sha`/`files`.
7. For files under `conf.d`, emit `fish -c 'emit <stem>_{install|update|uninstall}'` events.

## Concurrency

- `PEZ_JOBS` controls concurrency for `upgrade`, `uninstall`, and `prune`.
- `install` concurrency depends on how it is invoked:
  - With explicit targets (`install <targets...>`): clones run concurrently (unbounded), file copies run sequentially with duplicate‑path detection and warnings.
  - From `pez.toml` (no targets): processing is sequential and destination files are overwritten.

## Paths and Resolution

- Config dir precedence: `PEZ_CONFIG_DIR` → `__fish_config_dir` → `XDG_CONFIG_HOME/fish` → `~/.config/fish`.
- Data dir precedence: `PEZ_DATA_DIR` → `__fish_user_data_dir/pez` → `XDG_DATA_HOME/fish/pez` → `~/.local/share/fish/pez`.
- Copy destination: always the Fish config directory; overriding the target base via an env var is not supported.

## Upgrade Semantics

- Local sources are skipped.
- Non‑local plugins update to the latest commit on the remote default branch (remote HEAD).
- Selectors in `pez.toml` (version/branch/tag/commit) are not used by `upgrade`; they are honored on fresh installs and on `install --force`.
