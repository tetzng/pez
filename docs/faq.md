## FAQ

### Where does pez put files?

pez copies plugin files into your Fish config directory (`functions`, `completions`, `conf.d`, `themes`). See docs/configuration.md for directory precedence.

### Where are plugin repos cloned?

Under the pez data directory (by default `~/.local/share/fish/pez`). You can override via `PEZ_DATA_DIR`.

### Why doesn't `upgrade` change my plugin pinned by tag/branch in pez.toml?

`upgrade` updates non‑local plugins to the remote default branch HEAD and ignores selectors. Selectors are honored for fresh installs and `install --force`.

### How are duplicates handled when copying files?

- CLI targets (`pez install owner/repo ...`): duplicate destination paths are skipped with a warning.
- From `pez.toml` (`pez install` with no targets): existing files are overwritten.

### How do I uninstall everything not in pez.toml?

Run `pez prune`. Use `--dry-run` to preview and `--yes` to skip confirmation when `pez.toml` is empty.

### How do I use a local plugin?

Add `[[plugins]] path = "~/path/to/plugin"`. Local sources are not upgraded and are excluded from `list --outdated`.

### I installed the same repo twice with a different name — is that supported?

The lockfile deduplicates by repo/name. Prefer a single install per repo. If you need a custom display name, set `name = "..."` in the plugin spec.
