# Architecture

pez is a single Rust CLI. `src/main.rs` sets up logging, parses CLI arguments,
and dispatches each command through `src/cmd/`.

## Module Map

| Module | Responsibility |
| --- | --- |
| `src/cli.rs` | `clap` definitions for commands, arguments, and CLI target parsing. |
| `src/cmd/` | Command orchestration. Each command exposes `pub(crate) run`. |
| `src/config.rs` | Load, validate, and save `pez.toml`; convert config entries to install targets. |
| `src/lock_file.rs` | Load, validate, and save `pez-lock.toml`; resolve installed files. |
| `src/models.rs` | Shared types such as `PluginRepo`, `InstallTarget`, and `TargetDir`. |
| `src/resolver.rs` | Convert refs into selection semantics such as latest, version, branch, tag, and commit. |
| `src/git.rs` | Clone repositories, fetch refs, resolve selections, and checkout commits. |
| `src/utils.rs` | Path resolution, file copying, hook emitting, staged cleanup, and shared helpers. |
| `src/tests_support/` | Isolated test environment and log helpers. |

## Install Flow

1. Load or create `pez.toml` and `pez-lock.toml`.
2. Resolve CLI targets or config entries into `ResolvedInstallTarget` values.
3. Clone remote repos into the pez data directory. Local path sources skip clone.
4. Resolve the selected revision.
5. Copy supported fish assets into the target fish config directory.
6. Apply run-level duplicate destination checks on dedupe-enabled install paths.
7. For raw commands, emit out-of-process hook events for eligible `conf.d`
   files unless `PEZ_SUPPRESS_EMIT=1` is set.
8. Write installed state and copied file records to `pez-lock.toml`.

Explicit CLI installs clone concurrently and then copy files sequentially.
Config-driven installs are sequential. Fresh config-driven entries use direct
copying; explicit CLI installs and config-driven entries that copy an already
tracked plugin use dedupe-enabled copying.

## Upgrade Flow

1. Read the current lockfile.
2. Find matching config entries when selectors are present.
3. Skip local path sources.
4. Resolve the target remote revision.
5. Remove old copied files through staged cleanup.
6. Checkout the new commit, copy files, and update the lockfile.
7. Restore staged files when a later state write fails.

## Uninstall and Prune Flow

`uninstall` removes named plugins. `prune` removes plugins present in the
lockfile but absent from `pez.toml`.

Both flows use staged file and directory removals so failures can preserve or
restore state before the command reports an error.

## Path Model

| State | Default |
| --- | --- |
| Config and lockfile | `~/.config/fish` |
| Cached repos | `~/.local/share/fish/pez` |
| Copied plugin files | fish config directory |

Environment overrides are documented in [configuration](configuration.md).

## Hook Model

pez has two hook paths:

- Raw commands emit eligible events out of process. Raw `install` emits before
  the lockfile write; raw `upgrade` and `uninstall` emit after state updates
  succeed.
- `pez activate fish | source` installs a fish wrapper so hooks run in the
  current shell.

The activation wrapper suppresses duplicate raw emits by setting
`PEZ_SUPPRESS_EMIT=1`. It emits install and upgrade hooks in the current shell
after the raw command succeeds, and emits uninstall hooks before the raw
uninstall command while recorded files still exist.

## Documentation Boundaries

- User workflow: [README](../README.md), [getting started](getting-started.md)
- Exact command behavior: [commands](commands.md)
- Config, lockfile, paths, and environment: [configuration](configuration.md)
- Migration risk and fisher mapping: [migrate from fisher](migrate-from-fisher.md)
