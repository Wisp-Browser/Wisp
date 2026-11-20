# Wisp for iOS

Wisp for iPhone and iPad, written in Rust. It's a native UIKit app built with the
[objc2](https://github.com/madsmtm/objc2) crates, with no Swift, no Xcode project and no wry/tao.

## How it stays light

- **Engine:** pages use WebKit (`WKWebView`). iOS requires this of every browser, and the engine
  is part of the system, shared with Safari and other apps. Wisp itself is a ~1–2 MB binary.
- **At most 3 live pages.** Older background tabs are unloaded and reload from their address when
  you open them. On a low-memory warning, every page except the one on screen is dropped.
- **Nothing runs while idle.** There are no timers. The page script only watches `<head>` (for
  title changes) and is debounced. The New Tab clock wakes once a minute, only while visible.
- **Content blocker:** about 80 ad and tracker networks are blocked by WebKit's own content-rule
  engine, which is compiled once, cached by iOS, and adds no per-request work in Rust.

## What's in it

- **First-run setup**: theme (automatic/light/dark), layout (bottom or top bar), glass or flat,
  font (SF Pro, SF Rounded, New York serif, SF Mono — all built into iOS) and accent colour. You
  can run it again from Settings.
- **Liquid Glass**: on iOS 26 the bar buttons, the bookmarks bar and the menu sit on Apple's own
  glass material (`UIGlassEffect`). Older iOS uses the system blur. In the Glass style, pages run
  underneath the floating bar.
- **Bookmarks bar** next to the address bar. Add bookmarks from the menu (⋯ → Add bookmark) and
  manage them in Settings.
- Address bar with search, reload/stop, and full-screen search with "switch to tab" suggestions.
- Tab grid with close, close all (tap twice) and new tab. Pages cross-fade when you switch tabs.
- Menu: back/forward/reload, new tab, bookmark, copy link, request desktop site, ad blocker,
  settings.
- Settings: everything from setup, plus the New Tab wallpaper, clock and shortcuts, search engine
  and ad blocker.
- Swipe from the screen edge to go back or forward.
- Tabs, bookmarks and settings are saved between launches.
- Links to phone numbers, mail, maps and the App Store open in their apps.
- `target=_blank` links open in a new tab.

Not yet: history, downloads, private tabs, tab thumbnails, JavaScript `alert()` dialogs, sync
with desktop Wisp.

## Layout

| Path | What |
|---|---|
| `src/core.rs` | Tabs, settings, saving, blocker rules. Plain Rust with unit tests. |
| `src/app.rs` | UIKit/WebKit: app delegate, views, layout, page and interface messages. |
| `src/wk.rs` | `WKWebView` declaration for iOS (missing from objc2-web-kit). |
| `../src/omni.rs` | Address-bar logic, shared with desktop Wisp. |
| `ui/chrome.html` | The interface (bottom bar, sheets). |
| `ui/newtab.html` | The New Tab page. |
| `Info.plist`, `build-ipa.sh` | App bundle and packaging. |
| `icon.svg` → `AppIcon.png` | App icon source and its 1024px render. |

## Building

Building needs macOS with Xcode's command line tools:

```sh
bash ios/build-ipa.sh      # -> ios/target/ipa/Wisp-<version>-ios.ipa
```

On any OS, `cd ios && cargo test` runs the unit tests, and
`cargo check --target aarch64-apple-ios` type-checks the iOS code (no Mac needed).

Without a Mac, GitHub builds it: the **iOS** workflow (`.github/workflows/ios.yml`) runs on every
push that touches `ios/` and uploads `Wisp-ios` (the `.ipa`) as a workflow artifact.

## Installing on your iPhone from Windows

The `.ipa` from GitHub isn't signed for your phone. A sideloading tool signs it with your own
(free) Apple ID:

1. **Get the .ipa:** download `Wisp-<version>-iOS.ipa` from the
   [latest release](https://github.com/jakedev2/Wisp/releases/latest). Builds of unreleased
   changes are under **Actions → iOS** as the `Wisp-ios` artifact.
2. **Install Apple's device drivers:** install **iTunes** (the version from apple.com, not the
   Microsoft Store one) or the **Apple Devices** app. Then plug in the iPhone and tap **Trust** on
   the phone.
3. **Install [Sideloadly](https://sideloadly.io)** on Windows. Open it, drag the `.ipa` in, pick
   your iPhone, enter your Apple ID, and click **Start**.
4. **On the iPhone:**
   - iOS 16 or later: go to **Settings → Privacy & Security → Developer Mode**, turn it on, and
     restart.
   - Then go to **Settings → General → VPN & Device Management**, tap your Apple ID, and tap
     **Trust**.
5. Open **Wisp** from the home screen.

Free Apple IDs have limits: the app stops opening after **7 days** (run Sideloadly again to
refresh it), and you can have up to 3 sideloaded apps at once. A paid Apple Developer account
($99/year) extends this to a year and allows TestFlight.
