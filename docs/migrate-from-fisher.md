# Migrate from fisher to pez

This guide provides a low-risk migration path from `fisher` to `pez`.

## Recommended path

1. Enable activation in your current shell.

```fish
pez activate fish | source
```

2. Import `fish_plugins` into `pez.toml`.

```fish
pez migrate
```

3. Install migrated plugins.

```fish
pez install
```

4. Verify the result.

```fish
pez list --format table
pez doctor
```

5. Remove or disable fisher after verification.

## Common pitfalls

- `pez activate fish` is not enabled:
  `install`/`upgrade`/`uninstall` complete, but in-process `conf.d` events are not emitted in the current shell.
- `fisher` itself was removed during migration:
  this is expected; `jorgebucaran/fisher` is skipped by `pez migrate`.
- `conf.d` behavior differs before activation:
  without activation wrapper, hooks are emitted out-of-process and may not affect the current interactive shell session.

## What pez handles (and does not)

- Handled:
  - Reads `fish_plugins`, ignores blank/comment lines, and merges entries into `pez.toml`.
  - Copies plugin assets from `functions`, `completions`, `conf.d`, `themes`.
  - Tracks installed files and commits in `pez-lock.toml`.
- Not handled automatically:
  - Editing your `config.fish` to persist activation.
  - Removing fisher from your shell config.
  - Recovering custom manual edits in plugin-managed files.

## fisher and pez command mapping

| fisher | pez |
| --- | --- |
| `fisher install owner/repo` | `pez install owner/repo` |
| `fisher remove owner/repo` | `pez uninstall owner/repo` |
| `fisher update` | `pez upgrade` |
| `fisher list` | `pez list --format table` |
| `fisher` diagnostics (manual checks) | `pez doctor` |

## Rollback

If you want to go back to fisher:

1. Keep a backup of current `pez.toml` and `pez-lock.toml`.
2. Remove pez-managed plugin files if needed:

```fish
pez prune --force --yes
```

3. Remove or comment out `pez activate fish` from `config.fish`.
4. Reinstall and re-enable fisher.
5. Restore `fish_plugins` from backup and run fisher install flow.
