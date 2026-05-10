#!/usr/bin/env bash
set -euo pipefail

APP_NAME="Welly-rs"
VOLUME_NAME="Welly-rs"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BUNDLE_DIR="${REPO_ROOT}/target/release/bundle/macos"
APP_DIR="${BUNDLE_DIR}/${APP_NAME}.app"
DMG_ROOT="${BUNDLE_DIR}/dmg-root"
DMG_PATH="${BUNDLE_DIR}/${APP_NAME}.dmg"

"${SCRIPT_DIR}/build-macos-app.sh"

rm -rf "${DMG_ROOT}" "${DMG_PATH}"
mkdir -p "${DMG_ROOT}"

cp -R "${APP_DIR}" "${DMG_ROOT}/${APP_NAME}.app"
ln -s /Applications "${DMG_ROOT}/Applications"

hdiutil create \
    -volname "${VOLUME_NAME}" \
    -srcfolder "${DMG_ROOT}" \
    -ov \
    -format UDZO \
    "${DMG_PATH}"

echo "Built ${DMG_PATH}"
