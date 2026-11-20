//! Wisp for iPhone and iPad.
//!
//! A native UIKit app written in Rust (objc2). Pages use WebKit (WKWebView), which iOS requires of
//! every browser and which the system shares between apps, so Wisp itself stays tiny. The browser
//! interface (bottom bar, tab grid, menus) is a small transparent web view over a system blur.

#[path = "../../src/omni.rs"]
#[allow(dead_code)]
mod omni;

mod core;

#[cfg(target_os = "ios")]
mod app;
#[cfg(target_os = "ios")]
mod glass;
#[cfg(target_os = "ios")]
mod wk;

#[cfg(target_os = "ios")]
fn main() {
    app::run()
}

#[cfg(not(target_os = "ios"))]
fn main() {
    eprintln!("Wisp for iOS runs on iPhone and iPad: build with `--target aarch64-apple-ios` (see ios/README.md).");
}
