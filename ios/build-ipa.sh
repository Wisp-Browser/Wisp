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

# Home-screen icon: an asset catalog with the normal icon plus the dark and tinted variants
# (iOS 18+), which have transparent backgrounds so iOS draws just the logo on its own backdrop.
XC=$OUT/Assets.xcassets
SET=$XC/AppIcon.appiconset
mkdir -p "$SET"
cp AppIcon.png icons/AppIcon-Dark.png icons/AppIcon-Tinted.png "$SET/"
printf '{"info":{"author":"xcode","version":1}}' > "$XC/Contents.json"
cat > "$SET/Contents.json" <<'JSON'
{"images":[
 {"filename":"AppIcon.png","idiom":"universal","platform":"ios","size":"1024x1024"},
 {"appearances":[{"appearance":"luminosity","value":"dark"}],"filename":"AppIcon-Dark.png","idiom":"universal","platform":"ios","size":"1024x1024"},
 {"appearances":[{"appearance":"luminosity","value":"tinted"}],"filename":"AppIcon-Tinted.png","idiom":"universal","platform":"ios","size":"1024x1024"}
],"info":{"author":"xcode","version":1}}
JSON
xcrun actool "$XC" --compile "$APP" --platform iphoneos --minimum-deployment-target 15.0 --app-icon AppIcon   --target-device iphone --target-device ipad --output-partial-info-plist "$OUT/icons.plist" --output-format human-readable-text
/usr/libexec/PlistBuddy -c "Merge $OUT/icons.plist" "$APP/Info.plist"
ls "$APP"

codesign --force --sign - --timestamp=none "$APP"

IPA="Wisp-$VERSION-iOS.ipa"
(cd "$OUT" && zip -qry "$IPA" Payload)
echo "Built $OUT/$IPA ($(du -h "$OUT/$IPA" | cut -f1)); app binary $(du -h "$APP/Wisp" | cut -f1)"
