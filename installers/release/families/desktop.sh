#!/usr/bin/env bash

publish_linux_desktop() {
  local desktop_version=$1
  local dist_dir="${WORKERS_DIR}/dist/debian"
  local source basename arch deb_arch storage_relative_path destination

  if [[ ${SKIP_BUILD} -eq 0 ]]; then
    MANIFEED_DESKTOP_APP_VERSION="${desktop_version}" \
    MANIFEED_DESKTOP_DEB_VERSION="${desktop_version}-1" \
      bash "${WORKERS_DIR}/installers/debian/build-debs.sh"
  fi

  deb_arch=$(current_deb_arch)
  source=$(find_linux_desktop_package "${dist_dir}" "${desktop_version}" "${deb_arch}")
  if [[ -z "${source}" ]]; then
    printf 'No Linux desktop .deb found for version %s and arch %s under %s\n' \
      "${desktop_version}" "${deb_arch}" "${dist_dir}" >&2
    exit 1
  fi

  basename=$(basename "${source}")
  arch=$(current_release_arch)
  storage_relative_path="desktop/${basename}"
  destination="${STORAGE_ROOT}/${storage_relative_path}"

  copy_file "${source}" "${destination}"
  append_catalog_metadata \
    "${destination}" \
    "desktop" \
    "manifeed-workers-desktop" \
    "linux" \
    "${arch}" \
    "${desktop_version}" \
    "" \
    "" \
    "deb_package" \
    "public" \
    "${storage_relative_path}"
}

publish_macos_desktop() {
  local desktop_version=$1
  local source="${WORKERS_DIR}/dist/macos/Manifeed Workers.dmg"
  local artifact_name="Manifeed-Workers-${desktop_version}.dmg"
  local storage_relative_path="desktop/${artifact_name}"
  local destination="${STORAGE_ROOT}/${storage_relative_path}"

  if [[ ${SKIP_BUILD} -eq 0 ]]; then
    MANIFEED_DESKTOP_APP_VERSION="${desktop_version}" \
      bash "${WORKERS_DIR}/installers/macos/build-dmg.sh"
  fi

  if [[ ! -f "${source}" ]]; then
    printf 'No macOS desktop dmg found at %s\n' "${source}" >&2
    exit 1
  fi

  copy_file "${source}" "${destination}"
  append_catalog_metadata \
    "${destination}" \
    "desktop" \
    "workers_desktop_app" \
    "macos" \
    "aarch64" \
    "${desktop_version}" \
    "" \
    "" \
    "desktop_app" \
    "public" \
    "${storage_relative_path}"
}
