#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
WORKERS_DIR=$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd)
DIST_DIR="${WORKERS_DIR}/dist/macos"
APP_DIR="${DIST_DIR}/Manifeed Workers.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"
MANIFEST_HELPER="${WORKERS_DIR}/installers/release/read_manifest_value.py"

mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}"

APP_VERSION="${MANIFEED_DESKTOP_APP_VERSION:-}"
if [[ -z "${APP_VERSION}" ]]; then
  APP_VERSION=$(python3 "${MANIFEST_HELPER}" "${WORKERS_DIR}/worker-desktop/Cargo.toml" "package.version")
fi

MANIFEED_DESKTOP_APP_VERSION="${APP_VERSION}" \
  cargo build --release -p worker-desktop --manifest-path "${WORKERS_DIR}/Cargo.toml"

install -m 0755 \
  "${WORKERS_DIR}/target/release/manifeed-workers" \
  "${MACOS_DIR}/manifeed-workers"

cat > "${CONTENTS_DIR}/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>CFBundleDisplayName</key>
    <string>Manifeed Workers</string>
    <key>CFBundleExecutable</key>
    <string>manifeed-workers</string>
    <key>CFBundleIdentifier</key>
    <string>com.manifeed.workers</string>
    <key>CFBundleName</key>
    <string>Manifeed Workers</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${APP_VERSION}</string>
    <key>LSMinimumSystemVersion</key>
    <string>14.0</string>
  </dict>
</plist>
EOF

echo "Built ${APP_DIR}"
