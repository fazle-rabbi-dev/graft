# graft

![vibe coded](https://img.shields.io/badge/vibe-coded-ff69b4?style=for-the-badge)

A small Rust CLI that downloads **only a subfolder** of a GitHub repository into a target directory.

## Usage

CLI mode:

```bash
graft <owner/repo> <subpath> [dest] [--ref <ref>] [--list] [--token <token>]
```

- `owner/repo` — e.g. `vercel/next.js`
- `subpath` — folder inside the repo, e.g. `examples`
- `dest` — where to extract (defaults to repo name)
- `--ref` — branch/tag/commit (default `HEAD`)
- `--list` — print entries instead of downloading
- `--token` / `GITHUB_TOKEN` — for private repos

Interactive mode (double-click the exe, or run with no args): prompts for repo, subpath, destination, ref.

## Build

```bash
cargo build --release
# binary at target/release/graft.exe
```

## "install" equivalent

There is no `npm install`. Dependencies are managed by Cargo:

```bash
cargo build          # fetches crates + compiles
cargo run -- <args>  # build + run
```

## Make `graft` available globally

```bash
cargo install --path .
```

Installs `graft.exe` into `~/.cargo/bin` (usually already on PATH). Then run `graft ...` from anywhere.

Alternatively, copy `target/release/graft.exe` into any folder on PATH.
