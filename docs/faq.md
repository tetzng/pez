# FAQ

## Where does pez put plugin files?

By default, pez copies files into the fish config directory:
`functions`, `completions`, `conf.d`, and `themes`.

Use `PEZ_TARGET_DIR` to override the copy destination. See
[configuration](configuration.md) for full path precedence.

## Where are repositories cloned?

Under the pez data directory. The default is `~/.local/share/fish/pez`.

Use `PEZ_DATA_DIR` to override it.

## What does the lockfile track?

`pez-lock.toml` records each installed plugin's name, repo identifier, source,
installed commit, and copied files. Commands such as `uninstall`, `prune`,
`doctor`, and `files` use it as the installed-state record.

## How is load order determined?

pez copies files. fish decides when they load.

If a plugin needs a specific order, manage that through fish configuration or
plugin filenames.

## Why wasn't a plugin upgraded?

Check whether the plugin has a selector in `pez.toml`.

- `branch`, `tag`, `commit`, and `version` selectors are respected.
- Local path plugins are skipped.
- Unpinned remote plugins update to the remote default branch.

## How are duplicate files handled?

Explicit CLI installs, and config-driven installs when they copy a plugin
already tracked in `pez-lock.toml`, check duplicate destinations within the
current run. If a later plugin would write a claimed path, pez skips that
plugin's file copies. The plugin can still appear in `pez-lock.toml`; only
copied files are recorded, so that entry may have an empty `files` list.

Fresh config-driven entries are copied directly.

Use `pez doctor` to inspect duplicate destination issues.

## How do I see what a plugin installed?

```sh
pez files owner/repo
pez files owner/repo --dir conf.d
pez files --all
```

Use `--format json` for machine-readable output.

## How do I run conf.d hooks in the current shell?

Source activation:

```fish
pez activate fish | source
```

For persistence:

```fish
if status is-interactive
    pez activate fish | source
end
```

This wraps `pez` so install, upgrade, and uninstall hooks can affect the current
shell.

## How do I remove plugins no longer listed in pez.toml?

Preview first:

```sh
pez prune --dry-run
```

Then prune:

```sh
pez prune
```

Use `--yes` to confirm prompts in scripts.

## How do I use a local plugin?

In `pez.toml`:

```toml
[[plugins]]
path = "~/plugins/local-plugin"
```

Or from the CLI:

```sh
pez install ./local-plugin
```

CLI relative paths are saved as absolute paths. Local plugins are skipped by
`upgrade` and excluded from `list --outdated`.

## Can I install the same repo twice?

Not as separate managed entries. `pez.toml` is unique by repo, and
`pez-lock.toml` rejects duplicate source or name entries.

Use one entry per repo. Set `name = "..."` only when you need a different
display name.
