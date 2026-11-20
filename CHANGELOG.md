# Changelog

All notable changes to Wisp are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). The version lives in one place: `Cargo.toml`.

## [Unreleased]

## [0.2.0] - 2026-11-20

A full redesign of the interface. Features, settings and data are unchanged.

### Added
- **Layouts** (Settings → Appearance, and the first-run setup): *Classic* (tabs on top, toolbar below),
  *Compact* (tabs and address bar in one slim row) and *Vertical* (collapsible tab sidebar, slim toolbar).
  Switching applies instantly and is remembered.
- **Glass effects on/off.** Glass uses the Windows 11 Mica backdrop for the window blur; Flat uses solid,
  opaque surfaces. Wisp falls back to Flat by itself when Windows can't blur the window (older Windows,
  or “Transparency effects” turned off).
- A short first-run setup (theme, layout, glass) with clickable previews.

### Changed
- New visual design: frosted glass surfaces with a light top edge, pill-shaped address bar and buttons,
  12–20px rounded tabs and cards, one blue-violet accent, the system font, and one outline icon set.
- Design tokens shared by Glass and Flat (switching only swaps values), dark by default plus light.
- Animations are 100–200ms ease-out using only transform and opacity: tabs open, close and reorder
  smoothly, menus fade and scale in, layout and style switches crossfade. The system reduce-motion
  setting is respected (including the new tab page's animated background).
- Only open menus and popups use a GPU blur; the bars and sidebar rely on the OS backdrop.
- The old default accent colour is replaced by the new one; an accent you picked yourself is kept.
- Settings has an Appearance section; the More menu has icons and a light/dark switch.

### Fixed
- Menus and popups no longer get clipped by the page while their open animation plays.
- Settings no longer jumps back to the top whenever a page in the background updates.
- Pressing Enter on a highlighted address-bar suggestion opens that suggestion.
- The tab context menu and toasts stay visible above the page in every layout.
- The animated star background of the chrome no longer runs (and uses CPU) behind the page while idle.
- New tab widgets no longer show faint full-width bars behind them, and the speed dial no longer wraps early.

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
