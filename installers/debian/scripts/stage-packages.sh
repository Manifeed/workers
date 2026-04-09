#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 2 ]; then
  printf 'Usage: %s <source-root> <amd64|arm64>\n' "$0" >&2
  exit 1
fi

SOURCE_ROOT=$1
DEB_ARCH=$2

WORKSPACE_DIR=$(CDPATH= cd -- "${SOURCE_ROOT}/../.." && pwd)
ASSETS_DIR="${SOURCE_ROOT}/assets"
TARGET_RELEASE_DIR="${WORKSPACE_DIR}/target/release"
ICON_SOURCE="${WORKSPACE_DIR}/installers/assets/manifeed-workers.svg"

DESKTOP_PACKAGE="${SOURCE_ROOT}/debian/manifeed-workers-desktop"

write_wrapper() {
  local destination=$1
  local target=$2
  install -d "$(dirname "${destination}")"
  cat > "${destination}" <<EOF
#!/bin/sh
set -eu
exec "${target}" "\$@"
EOF
  chmod 0755 "${destination}"
}

rm -rf "${DESKTOP_PACKAGE}"
install -d "${DESKTOP_PACKAGE}"

install -Dm0755 "${TARGET_RELEASE_DIR}/manifeed-workers" \
  "${DESKTOP_PACKAGE}/usr/lib/manifeed/desktop/manifeed-workers"
write_wrapper "${DESKTOP_PACKAGE}/usr/bin/manifeed-workers" "/usr/lib/manifeed/desktop/manifeed-workers"
install -Dm0644 "${ASSETS_DIR}/manifeed-workers.desktop" \
  "${DESKTOP_PACKAGE}/usr/share/applications/manifeed-workers.desktop"
install -Dm0644 "${ICON_SOURCE}" \
  "${DESKTOP_PACKAGE}/usr/share/icons/hicolor/scalable/apps/manifeed-workers.svg"

install -Dm0644 /dev/null "${DESKTOP_PACKAGE}/usr/share/doc/manifeed-workers-desktop/README.Debian"
cat > "${DESKTOP_PACKAGE}/usr/share/doc/manifeed-workers-desktop/README.Debian" <<'EOF'
Install this single Debian package, launch `manifeed-workers`, then install and manage RSS and Embedding workers directly from the desktop app.
EOF

printf 'Staged desktop package for %s\n' "${DEB_ARCH}"
