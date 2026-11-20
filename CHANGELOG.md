# Changelog

All notable changes to Wisp are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). The version lives in one place: `Cargo.toml`.

## [Unreleased]

## [0.1.0] - 2026-11-19

First public build.

### Added
- Frameless glass window on the system WebView2 (no bundled Chromium), custom Wisp logo and icon.
- Vertical tabs with drag-to-reorder, pinned tabs, duplicate, mute, hibernation of idle tabs, and session restore.
- Spaces: named, colour-coded workspaces with their own tabs, pins, wallpaper and cookie profile.
- Split screen with 2 to 4 panes and resizable dividers.
- Omnibox with search-engine prefixes, suggestions, and a command palette (Ctrl+K).
- Customizable new tab page: wallpapers (gallery, upload, gradients), animated starfield/particles, widgets.
- AI sidebar: Gemini, Groq and OpenRouter with your own key (kept in the Windows Credential Manager),
  local Ollama, or the web apps for Claude, ChatGPT and Gemini. Page-aware actions and per-space chats.
- Ad and tracker blocker with a Rust filter-list engine and per-site switch, HTTPS-only mode,
  bookmarks, history, downloads, find in page, zoom, reader mode, picture-in-picture, private windows,
  bookmark import (Chrome, Edge, Brave, Firefox, HTML), rebindable shortcuts and a Settings page.
- Per-user Windows installer with optional default-browser registration and optional Ollama install,
  and an uninstaller that asks whether to keep your data.
- Automatic updates from GitHub Releases with SHA-256 verification, restart-to-apply, and automatic
  rollback if a new version keeps failing to start.
- True full screen (F11 and video fullscreen).
