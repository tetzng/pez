# Migrate from fisher

pez can import fisher's `fish_plugins` into `pez.toml`. Keep the migration
reversible until `pez doctor` reports a setup you trust.

## Recommended Flow

1. Back up your fish config.
2. Enable current-shell activation.

```fish
pez activate fish | source
```

3. Preview the import.

```fish
pez migrate --dry-run
```

4. Import `fish_plugins`.

```fish
pez migrate
```

5. Install and verify.

```fish
pez install
pez list --format table
pez doctor
```

6. Remove or disable fisher only after verification.

## Faster Flow

Use `--install` when `pez migrate --dry-run` shows entries that do not need
manual edits before install:

```fish
pez activate fish | source
pez migrate --install
pez list --format table
pez doctor
```

## What migrate does

- Reads fisher's `fish_plugins`.
- Ignores blank lines and comments.
- Skips `jorgebucaran/fisher`.
- Merges imported entries into `pez.toml` by default.
- Preserves supported ref suffixes such as `owner/repo@2.0.0`,
  `owner/repo@tag:v1`, `owner/repo@branch:main`, and
  `owner/repo@commit:<sha>`.

Use `--force` only when you want the imported list to replace the current
`pez.toml` plugin list.

## What migrate does not do

- It does not edit `config.fish` to persist activation.
- It does not remove fisher from your shell config.
- It does not recover manual edits inside plugin-managed files.
- It does not sandbox or verify plugin code.

## Command Mapping

| fisher | pez |
| --- | --- |
| `fisher install owner/repo` | `pez install owner/repo` |
| `fisher remove owner/repo` | `pez uninstall owner/repo` |
| `fisher update` | `pez upgrade` |
| `fisher list` | `pez list --format table` |
| Manual checks | `pez doctor` |

## Common Pitfalls

- Without `pez activate fish | source`, install and upgrade still run, but
  hooks may not affect the current interactive shell.
- fisher itself being absent from `pez.toml` is expected. `pez migrate` skips it.
- URL-style fisher entries with ambiguous `@ref` suffixes may be ignored. Convert
  them to shorthand form or write explicit `url` entries in `pez.toml`.
- SCP-style SSH entries such as `git@host:owner/repo.git` can be imported, but
  `pez.toml` treats scheme-less `url` values as HTTPS. Before running
  `pez install` or `pez migrate --install`, convert them to
  `ssh://git@host/owner/repo.git` or `repo = "host/owner/repo"`.

## Rollback

1. Keep the backed-up fisher files until the migration is verified.
2. Remove migrated entries from `pez.toml`, or uninstall the migrated repos
   explicitly.

```fish
pez uninstall owner/repo
```

3. If you removed entries from `pez.toml`, prune the lockfile-only plugins.

```fish
pez prune --force --yes
```

4. Remove or comment out `pez activate fish` from `config.fish`.
5. Re-enable fisher.
6. Restore `fish_plugins` from backup and run fisher's install flow.
