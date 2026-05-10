#!/usr/bin/env bash
set -euo pipefail

APP_NAME="Welly-rs"
BUNDLE_ID="net.newsmth.welly-rs"
CRATE_NAME="welly-rs"
ICON_NAME="Welly-rs"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TARGET_DIR="${REPO_ROOT}/target/release"
BUNDLE_DIR="${TARGET_DIR}/bundle/macos"
APP_DIR="${BUNDLE_DIR}/${APP_NAME}.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"
EXECUTABLE_PATH="${MACOS_DIR}/${CRATE_NAME}"
ICONSET_DIR="${BUNDLE_DIR}/${ICON_NAME}.iconset"
ICON_PATH="${RESOURCES_DIR}/${ICON_NAME}.icns"

cd "${REPO_ROOT}"

cargo build --release

rm -rf "${APP_DIR}"
mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}"

cp "${TARGET_DIR}/${CRATE_NAME}" "${EXECUTABLE_PATH}"
chmod +x "${EXECUTABLE_PATH}"

rm -rf "${ICONSET_DIR}"
mkdir -p "${ICONSET_DIR}"

python3 "${SCRIPT_DIR}/make-app-icons.py" "${ICONSET_DIR}" "${ICON_PATH}"

cat > "${CONTENTS_DIR}/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIconFile</key>
    <string>${ICON_NAME}</string>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleExecutable</key>
    <string>${CRATE_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSHumanReadableCopyright</key>
    <string>GPL-3.0-or-later</string>
</dict>
</plist>
PLIST

codesign --force --sign - "${APP_DIR}"

echo "Built ${APP_DIR}"
