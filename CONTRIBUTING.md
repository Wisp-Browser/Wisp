# Contributing to Wisp

Thanks for helping! Wisp is a small project, so the process is light.

## Ground rules

- **Windows only.** Wisp uses WebView2. Linux builds exist purely so CI can compile and test the shared logic.
- Keep it **light**: no new heavy dependencies without a good reason, and nothing that adds idle CPU or memory.
- UI changes should match the existing glass style (see `ui/`). Include a screenshot in the pull request.

## Setting up

1. Install Rust (https://rustup.rs) with the MSVC toolchain and the "Desktop development with C++" build tools.
2. WebView2 Runtime (already on Windows 11).
3. `cargo run` runs a debug build. `cargo run --release` builds what we ship.

## Before you open a pull request

```
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs the same checks on Windows and Linux.

## How the code is laid out

| Path | What it does |
| --- | --- |
| `src/main.rs` | Window, tabs, spaces, webview creation, and the message bridge to the UI |
| `src/blocker.rs` | Filter-list engine and the WebView2 request hook |
| `src/library.rs` | Bookmarks, history, downloads, bookmark import |
| `src/ai.rs` | AI providers, streaming, keychain |
| `src/updater.rs` | Auto-update: feed, download, SHA-256, swap, rollback |
| `src/instance.rs` | Single instance and "open this link" hand-off |
| `src/layout.rs` | Split-screen geometry |
| `ui/` | The interface: plain HTML, CSS and JavaScript (served from inside the binary) |
| `installer/` | NSIS installer and uninstaller |
| `tools/gen_icons.py` | Regenerates icons and installer artwork from the logo |

The UI talks to Rust with `window.ipc.postMessage(JSON)` and Rust answers with `evaluate_script`.
Web pages can only send a tiny allowed set of messages; everything else requires Wisp's own origin.

## Releases

See "Releasing" in the README. In short: bump the version in `Cargo.toml`, update `CHANGELOG.md`,
tag `vX.Y.Z`, push the tag.
