#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
WORKERS_DIR=$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd)
DIST_DIR="${WORKERS_DIR}/dist/macos"
APP_DIR="${DIST_DIR}/Manifeed Workers.app"
DMG_PATH="${DIST_DIR}/Manifeed Workers.dmg"

"${SCRIPT_DIR}/build-app.sh"

rm -f "${DMG_PATH}"
hdiutil create \
  -volname "Manifeed Workers" \
  -srcfolder "${APP_DIR}" \
  -ov \
  -format UDZO \
  "${DMG_PATH}"

if [[ -n "${APPLE_DEVELOPER_ID:-}" ]]; then
  codesign --force --sign "${APPLE_DEVELOPER_ID}" "${APP_DIR}"
  codesign --force --sign "${APPLE_DEVELOPER_ID}" "${DMG_PATH}"
fi

echo "Built ${DMG_PATH}"
