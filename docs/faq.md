## FAQ

### Where does pez put files?

pez copies plugin files into your Fish config directory (`functions`, `completions`, `conf.d`, `themes`). See docs/configuration.md for directory precedence.

### How is load order determined?

pez only copies files into the Fish config directories; Fish determines when and how they are loaded. If you need a specific order, manage it in your Fish configuration.

### Where are plugin repos cloned?

Under the pez data directory (by default `~/.local/share/fish/pez`). You can override via `PEZ_DATA_DIR`.

### Why doesn't `upgrade` change my plugin pinned by tag/branch in pez.toml?

`upgrade` respects selectors defined in `pez.toml`. If you pin a plugin to a specific `branch`, `tag`, `commit`, or `version`, `upgrade` resolves against that selector. When no selector is set, `upgrade` updates to the latest commit on the remote default branch (remote HEAD).

### How are duplicates handled when copying files?

- Duplicate destination paths are detected for both CLI targets and installs from `pez.toml`. Conflicting plugins are skipped with a warning to avoid overwriting existing files.

### How do I list the files installed by a plugin?

Use `pez files owner/repo` for a single plugin or `pez files --all` for everything. Add `--dir conf.d` to filter to conf.d scripts.

### How do I run conf.d hooks in my current shell?

Source the activation script: `pez activate fish | source`. For persistence, place it in `~/.config/fish/config.fish` inside `if status is-interactive ... end`. This wraps `pez` so `install`/`upgrade`/`uninstall` source the affected conf.d files and emit events in the current shell.

### How do I uninstall everything not in pez.toml?

Run `pez prune`. Use `--dry-run` to preview and `--yes` to skip confirmation when `pez.toml` has no `[[plugins]]` entries.

### How do I use a local plugin?

Add `[[plugins]] path = "~/path/to/plugin"`. Local sources are not upgraded and are excluded from `list --outdated`.

### I installed the same repo twice with a different name — is that supported?

Not supported: `pez.toml` entries are unique by repo, and the lockfile also enforces unique source/name. Prefer a single install per repo. If you need a custom display name, set `name = "..."` in the plugin spec.
