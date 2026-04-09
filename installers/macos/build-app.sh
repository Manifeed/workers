#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
WORKERS_DIR=$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd)
DIST_DIR="${WORKERS_DIR}/dist/macos"
APP_DIR="${DIST_DIR}/Manifeed Workers.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}"

APP_VERSION="${MANIFEED_DESKTOP_APP_VERSION:-}"
if [[ -z "${APP_VERSION}" ]]; then
APP_VERSION=$(python3 - "${WORKERS_DIR}/worker-source-embedding-desktop/Cargo.toml" <<'PY'
import tomllib
import sys
from pathlib import Path

path = Path(sys.argv[1])
data = tomllib.loads(path.read_text(encoding="utf-8"))
print(data["package"]["version"])
PY
)
fi

MANIFEED_DESKTOP_APP_VERSION="${APP_VERSION}" \
  cargo build --release -p worker-source-embedding-desktop --manifest-path "${WORKERS_DIR}/Cargo.toml"

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
