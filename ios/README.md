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

## What's in this first version

- Bottom bar: back, address/search, reload/stop, tab count, menu. The bar sits on the system blur
  material, so it's frosted glass.
- Full-screen search, with "switch to tab" suggestions.
- Tab grid: open, close, close all (tap twice), new tab.
- Menu: back/forward/reload, new tab, copy link, request desktop site, ad blocker, settings
  (search engine).
- Swipe from the screen edge to go back or forward.
- New Tab page with clock, search and shortcuts.
- Tabs and settings are saved between launches.
- Links to phone numbers, mail, maps and the App Store open in their apps.
- `target=_blank` links open in a new tab.
- Light and dark mode follow the system. Works in portrait and landscape, on iPhone and iPad.

Not yet: history, bookmarks, downloads, private tabs, tab thumbnails, JavaScript `alert()` dialogs,
sync with desktop Wisp.

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

1. **Get the .ipa:** on GitHub, open **Actions → iOS → the latest green run**. Download
   **Wisp-ios** under *Artifacts* and unzip it to get `Wisp-…-ios.ipa`.
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
