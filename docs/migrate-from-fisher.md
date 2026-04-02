# Migrate from fisher to pez

This guide provides a low-risk migration path from `fisher` to `pez`.

## Recommended path

1. Decide whether you want fisher-like runtime hooks.

By default, pez keeps shell hooks disabled:

```toml
[shell_hooks]
emit = false
source = false
```

If you want current-shell `conf.d` sourcing and event emission similar to
fisher, enable them first:

```toml
[shell_hooks]
emit = true
source = true
```

2. Enable activation in your current shell if you turned on `shell_hooks.source`.

```fish
pez activate fish | source
```

3. Import `fish_plugins` into `pez.toml`.

```fish
pez migrate
```

4. Install migrated plugins.

```fish
pez install
```

5. Verify the result.

```fish
pez list --format table
pez doctor
```

6. Remove or disable fisher after verification.

## Common pitfalls

- `pez activate fish` alone does not enable hooks:
  the wrapper reads `pez.toml` at runtime; if `shell_hooks.emit` / `shell_hooks.source` remain `false`, no current-shell hook actions run.
- `fisher` itself was removed during migration:
  this is expected; `jorgebucaran/fisher` is skipped by `pez migrate`.
- `conf.d` behavior differs before activation:
  with default settings, pez only copies plugin files. If you enable `shell_hooks.emit` without `shell_hooks.source`, events run out-of-process and may not affect the current interactive shell session.

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
