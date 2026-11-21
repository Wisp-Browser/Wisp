# Changelog

All notable changes to Wisp are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). The version lives in one place: `Cargo.toml`.

## [Unreleased]

## [0.2.8] - 2026-11-20

### Added
- Chrome's keyboard shortcuts:
  - Ctrl+Shift+T reopens closed tabs (the last 25, in their own Space).
  - Ctrl+1…8 / Ctrl+9 go to a tab / the last tab, and Ctrl+PgDn / Ctrl+PgUp switch tabs.
  - Alt+Left / Right / Home go back, forward and to the New Tab page.
  - F5 reloads, and Ctrl+Shift+R / Shift+F5 reload ignoring the cache.
  - Alt+D / F6 / Ctrl+E focus the address bar, and Ctrl+Enter adds www. and .com.
  - Ctrl+Shift+D bookmarks all tabs, Ctrl+Shift+B toggles the bookmarks bar, and Ctrl+J opens
    downloads.
  - Ctrl+G finds the next match, Ctrl+U shows the page source, and F12 / Ctrl+Shift+I open
    developer tools.
  - Ctrl+Shift+Delete clears browsing data, Alt+F opens the menu, and Ctrl+Shift+W closes the
    window.
  - A command can have several keys.
- Import history from Chrome, Edge, Brave and Firefox, and from Google Takeout
  (`Chrome/History.json`). Settings → Bookmarks & import.

### Changed
- To match Chrome:
  - Wisp AI moves to Ctrl+Shift+Space (Ctrl+J is now downloads).
  - Reader mode moves to F9 (Ctrl+Shift+R is now a hard reload).
  - The per-site ad blocker toggle has no default shortcut.
- Custom shortcuts you set are kept.
- Switching tabs updates the address bar even while it has focus.

## [0.2.7] - 2026-11-20

### Added
- The Settings window can be dragged by its title, a section heading or empty space in its
  sidebar. Double-click one of those to put it back in the middle.

### Fixed
- New tabs no longer flash Wisp's default wallpaper, font and colour before switching to yours.
  The ready-made New Tab page now gets your look before it first draws, and stays up to date.
- AI panel: the API key box no longer overlaps the "Get a free key" link (same for the Ollama
  address box).
- iPhone: the glass bar buttons no longer start out under the status bar (or over the home
  indicator) when the app opens.

## [0.2.6] - 2026-11-20

### Added
- Wisp reopens how you left it: window size and position, maximized, and full screen. If that
  monitor is gone, it opens at the default size instead.
- Right-click a bookmark on the bookmarks bar to open it in a new tab, copy its address, edit its
  name and address, or delete it.

### Fixed
- Menus are fully solid, so their text is easy to read over bright pages.
- iPhone: the New Tab wallpaper now fills the screen behind the status bar and the glass bar (no
  more dark band), and shortcut names stay on one line.

## [0.2.5] - 2026-11-20

### Changed (iPhone and iPad)
- Real Liquid Glass on iOS 26:
  - Back, tabs and the menu button are native glass buttons.
  - The menu is a native iOS menu that morphs out of the button.
  - All glass (bar, address pill, bookmarks bar) shares one glass container, and appears and
    disappears with the system's materialize animation.
- The menu panel now grows from the menu button (it came out of the back button before).
- Bigger logo on the app icon, plus Dark and Tinted icons (iOS 18+) with transparent backgrounds.

## [0.2.4] - 2026-11-20

### Added
- Bookmarks bar under the address bar, in every layout. Click opens a bookmark in the tab, and
  middle- or Ctrl-click opens it in a new tab. Scroll it sideways with the wheel. It shows once you
  have a bookmark, and can be switched off in Settings → Appearance.
- **Wisp for iPhone and iPad (preview)**, attached to each release as `Wisp-<version>-iOS.ipa`. It's
  a native Rust app on iOS's WebKit engine:
  - First-run setup: theme, layout (bottom or top bar), glass or flat, font, accent colour.
  - Customization in Settings: New Tab wallpaper, clock and shortcuts, search engine, ad blocker.
  - Apple's Liquid Glass (iOS 26) behind the bar buttons and menu; the system blur on older iOS.
  - A bookmarks bar, tab grid, full-screen search and a content blocker.

### Fixed
- New Tab: gradient wallpaper swatches no longer show a frame at their edges.

## [0.2.3] - 2026-11-20

Includes everything from 0.2.2-beta.1 and 0.2.2-beta.2 (smooth corners, Settings clipping fix,
font setting).

### Added
- Memory saver (Settings → Tabs): tabs you aren't looking at are unloaded after 30 seconds by default
  (Maximum), 5 minutes (Balanced), 30 minutes (Moderate) or never (Off). They reload when clicked, at the
  same scroll position. Tabs playing audio or video, tabs with text typed into a form, and all tabs
  while a download is running are kept.
- Hidden tabs lower their memory target right away, before they are unloaded.
- Wisp hands memory its engine is no longer using back to Windows after tabs unload, and fully while
  it is in the background. With 15 tabs open, Wisp's total memory went from about 2.2 GB to about
  130–250 MB in use and about 135 MB in the background (depends on the open page).
- `WISP_DATA_DIR` environment variable runs Wisp on a separate data folder.

### Changed
- The old "Hibernate inactive tabs" minutes setting is replaced by Memory saver; existing installs move
  to Maximum.

## [0.2.2-beta.2] - 2026-11-20

### Added
- Font setting in Settings → Appearance and as a step in the first-run setup: Inter (new default),
  Nunito, Segoe UI, Serif and Mono, applied instantly. Inter and Nunito are bundled (SIL OFL).

### Fixed
- Menus and dialogs over a page no longer show faint squares at their rounded corners (their drop shadow
  was being cut off square).

## [0.2.2-beta.1] - 2026-11-20

### Fixed
- Page corners are now smooth (anti-aliased) instead of pixelated. The chrome now sits above the pages and
  draws their rounded corners itself, with clicks still going straight to the page.
- Menus, popups and dialogs over a page have smooth corners too, and the page no longer gets cut out
  behind them.
- The Settings dialog is no longer clipped with a notch where it reaches above the page area.
- Menus are more opaque so their text stays readable over busy pages.

## [0.2.1] - 2026-11-20

### Added
- **Wisp AI (free)**: a built-in free model (GPT-OSS 20B via Pollinations.ai) that works with no sign-in or
  API key. It is the default AI provider; the key-based providers, Ollama and the web apps are still available.

### Fixed
- Removed the dark, jagged outline drawn around the page.

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
