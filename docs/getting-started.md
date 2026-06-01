# Getting Started

This guide gets from a new `pez.toml` to an installed, inspectable fish plugin
setup.

## 1. Create the config file

```sh
pez init
```

`pez.toml` is created under the fish config directory unless `PEZ_CONFIG_DIR`
overrides it.

## 2. Add plugins

Use one source per plugin.

GitHub shorthand:

```toml
[[plugins]]
repo = "owner/repo"
```

Another Git host:

```toml
[[plugins]]
repo = "gitlab.com/owner/repo"
branch = "main"
```

Full Git URL:

```toml
[[plugins]]
url = "https://example.com/owner/repo"
tag = "v1.2.3"
```

Local directory:

```toml
[[plugins]]
path = "~/plugins/local-plugin"
```

Selectors are optional. Use at most one of `version`, `branch`, `tag`, or
`commit`.

## 3. Install and verify

```sh
pez install
pez list --format table
pez doctor
```

`pez install` copies supported fish assets into the target fish config
directories and records installed files in `pez-lock.toml`.

## 4. Install directly from the CLI

CLI installs also update `pez.toml`, so future `pez install` runs remain in
sync.

```sh
pez install owner/repo
pez install owner/repo@v3
pez install gitlab.com/owner/repo@branch:main
pez install https://example.com/owner/repo.git
pez install ./local-plugin
```

For shorthand targets, plain `@ref` is treated as `version`. Use
`@branch:name`, `@tag:name`, or `@commit:sha` when the selector type matters.
Full URLs are not parsed for `@ref`; put selectors in `pez.toml` for URL
sources.

## 5. Enable optional shell integration

Completions:

```sh
pez completions fish > ~/.config/fish/completions/pez.fish
```

Current-shell activation for `conf.d` hooks:

```fish
pez activate fish | source
```

To persist activation, add it to `~/.config/fish/config.fish` inside an
interactive-shell guard:

```fish
if status is-interactive
    pez activate fish | source
end
```

## Daily Commands

| Task | Command |
| --- | --- |
| Install configured plugins | `pez install` |
| Install one plugin | `pez install owner/repo` |
| Update plugins | `pez upgrade` |
| Show installed plugins | `pez list --format table` |
| Show outdated remote plugins | `pez list --outdated --format table` |
| Show installed files | `pez files --all` |
| Diagnose setup state | `pez doctor` |
| Remove lockfile-only plugins | `pez prune --dry-run` |

Logs default to info. Use `-vv` for debug logs. Use `--jobs <N>` to override
parallel work where the command supports it.
