#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
OUTPUT_DIR=$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd)/dist/debian
CHANGELOG_PATH="${SCRIPT_DIR}/debian/changelog"
BACKUP_CHANGELOG_PATH=""

command -v dpkg-buildpackage >/dev/null 2>&1 || {
  printf 'dpkg-buildpackage is required to build Debian packages.\n' >&2
  exit 1
}
command -v dh >/dev/null 2>&1 || {
  printf 'debhelper (dh) is required to build Debian packages.\n' >&2
  exit 1
}
command -v cargo >/dev/null 2>&1 || {
  printf 'cargo is required to build Debian packages.\n' >&2
  exit 1
}
command -v rustc >/dev/null 2>&1 || {
  printf 'rustc is required to build Debian packages.\n' >&2
  exit 1
}

mkdir -p "${OUTPUT_DIR}"
rm -f "${OUTPUT_DIR}"/*.deb "${OUTPUT_DIR}"/*.ddeb "${OUTPUT_DIR}"/*.changes "${OUTPUT_DIR}"/*.buildinfo

if [[ -n "${MANIFEED_DESKTOP_DEB_VERSION:-}" ]]; then
  BACKUP_CHANGELOG_PATH=$(mktemp)
  cp -f "${CHANGELOG_PATH}" "${BACKUP_CHANGELOG_PATH}"
  trap 'if [[ -n "${BACKUP_CHANGELOG_PATH}" && -f "${BACKUP_CHANGELOG_PATH}" ]]; then mv -f "${BACKUP_CHANGELOG_PATH}" "${CHANGELOG_PATH}"; fi' EXIT
  cat > "${CHANGELOG_PATH}" <<EOF
manifeed-workers (${MANIFEED_DESKTOP_DEB_VERSION}) unstable; urgency=medium

  * Automated Debian packaging for Manifeed workers.

 -- Manifeed Maintainers <maintainers@manifeed.local>  $(LC_ALL=C date -Ru)
EOF
fi

if [[ -n "${MANIFEED_DESKTOP_APP_VERSION:-}" ]]; then
  export MANIFEED_DESKTOP_APP_VERSION
fi

(
  cd "${SCRIPT_DIR}"
  dpkg-buildpackage -us -uc -b
)

find "${SCRIPT_DIR}/.." -maxdepth 1 -type f \
  \( -name 'manifeed-workers-desktop_*.deb' -o -name '*.changes' -o -name '*.buildinfo' \) \
  -exec cp -f {} "${OUTPUT_DIR}/" \;

printf 'Debian artifacts copied to %s\n' "${OUTPUT_DIR}"
