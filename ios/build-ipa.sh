#!/usr/bin/env bash
# Builds Wisp for iOS and packages it as an .ipa (needs macOS with Xcode's command line tools).
# The .ipa is ad-hoc signed only: install it with a sideloading tool (see ios/README.md), which
# signs it with your own Apple ID.
set -euo pipefail
cd "$(dirname "$0")"

export IPHONEOS_DEPLOYMENT_TARGET=15.0
rustup target add aarch64-apple-ios >/dev/null
cargo build --release --target aarch64-apple-ios

# Releases pass Wisp's version (e.g. 0.2.4-beta.1); iOS bundle versions must be plain numbers.
VERSION=${WISP_VERSION:-$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)}
PLIST_VERSION=${VERSION%%-*}
BUILD=${GITHUB_RUN_NUMBER:-1}
OUT=target/ipa
APP=$OUT/Payload/Wisp.app
rm -rf "$OUT"
mkdir -p "$APP"

cp target/aarch64-apple-ios/release/Wisp "$APP/Wisp"
sed -e "s/__VERSION__/$PLIST_VERSION/" -e "s/__BUILD__/$BUILD/" Info.plist > "$APP/Info.plist"
plutil -lint "$APP/Info.plist"
printf 'APPL????' > "$APP/PkgInfo"

# Home-screen icons, scaled down from the 1024px icon (rendered from icon.svg).
ICON=AppIcon.png
sips -z 120 120 "$ICON" --out "$APP/AppIcon60x60@2x.png" >/dev/null
sips -z 180 180 "$ICON" --out "$APP/AppIcon60x60@3x.png" >/dev/null
sips -z 152 152 "$ICON" --out "$APP/AppIcon76x76@2x~ipad.png" >/dev/null
sips -z 167 167 "$ICON" --out "$APP/AppIcon83.5x83.5@2x~ipad.png" >/dev/null

codesign --force --sign - --timestamp=none "$APP"

IPA="Wisp-$VERSION-iOS.ipa"
(cd "$OUT" && zip -qry "$IPA" Payload)
echo "Built $OUT/$IPA ($(du -h "$OUT/$IPA" | cut -f1)); app binary $(du -h "$APP/Wisp" | cut -f1)"
