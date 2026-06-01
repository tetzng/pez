# Install and Build

## Requirements

- A prebuilt `pez` binary does not require Cargo.
- Cargo is required for `cargo install` and source builds.
- fish is required to use installed fish plugins and to source activation code.
- Nix is optional.

## Install a Release Binary

Download the asset for your platform from
[GitHub Releases](https://github.com/tetzng/pez/releases). Release assets may
be archives, so extract the downloaded file first, then make the `pez` binary
executable and place it on your `PATH`.

```sh
tar -xf pez-*.tar.*
chmod +x path/to/pez
path/to/pez --version
```

Asset names vary by release and platform, so use the release page as the source
of truth. Use the archive layout from the extracted asset when replacing
`path/to/pez`.

## Install with Cargo

When the crate is published to crates.io:

```sh
cargo install pez
```

From this repository checkout:

```sh
cargo install --path .
```

## Run with Nix

```sh
nix run github:tetzng/pez -- --version
```

If flakes are not enabled globally:

```sh
nix --extra-experimental-features 'nix-command flakes' run github:tetzng/pez -- --version
```

## Build from Source

```sh
cargo build --release
./target/release/pez --version
```

With Nix:

```sh
nix build .#
./result/bin/pez --version
```

## Development Shell

```sh
nix develop .#
```

The Nix shell provides the Rust toolchain, `rustfmt`, `clippy`, and fish from
the pinned flake input. Outside Nix, use your normal Rust toolchain.

## Shell Files

Install completions:

```sh
pez completions fish > ~/.config/fish/completions/pez.fish
```

Activate current-shell hooks:

```fish
pez activate fish | source
```

Persist activation from `~/.config/fish/config.fish`:

```fish
if status is-interactive
    pez activate fish | source
end
```

## Release Notes for Maintainers

Tagged `v*.*.*` releases are built by CI and distributed through GitHub
Releases. Do not hand-edit cargo-dist generated workflow output; change the
generator configuration instead.
