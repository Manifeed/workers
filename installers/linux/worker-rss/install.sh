#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_WORKERS_DIR=$(CDPATH= cd -- "${SCRIPT_DIR}/../../.." && pwd)
REPO_TARGET_RELEASE_DIR="${REPO_WORKERS_DIR}/target/release"

WORKER_BINARY_NAME="worker-rss"
DESKTOP_BINARY_NAME="manifeed-workers"

XDG_CONFIG_HOME_VALUE="${XDG_CONFIG_HOME:-${HOME}/.config}"
XDG_DATA_HOME_VALUE="${XDG_DATA_HOME:-${HOME}/.local/share}"

CONFIG_DIR="${XDG_CONFIG_HOME_VALUE}/manifeed"
CONFIG_PATH="${CONFIG_DIR}/workers.json"
DATA_DIR="${XDG_DATA_HOME_VALUE}/manifeed"
WORKER_INSTALL_DIR="${DATA_DIR}/rss"
DESKTOP_INSTALL_DIR="${DATA_DIR}/desktop"
BIN_DIR="${HOME}/.local/bin"
APPLICATIONS_DIR="${HOME}/.local/share/applications"
ICONS_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"
ICON_SOURCE_PATH="${SCRIPT_DIR}/manifeed-workers.svg"
FALLBACK_ICON_SOURCE_PATH="${SCRIPT_DIR}/../manifeed-workers.svg"

DEFAULT_WORKER_BINARY_PATH="${SCRIPT_DIR}/${WORKER_BINARY_NAME}"
DEFAULT_DESKTOP_BINARY_PATH="${SCRIPT_DIR}/${DESKTOP_BINARY_NAME}"
FALLBACK_WORKER_BINARY_PATH="${REPO_TARGET_RELEASE_DIR}/${WORKER_BINARY_NAME}"
FALLBACK_DESKTOP_BINARY_PATH="${REPO_TARGET_RELEASE_DIR}/manifeed-workers"

UI_MODE="auto"
NON_INTERACTIVE=0
INSTALL_SERVICE=0
WORKER_BINARY_PATH="${DEFAULT_WORKER_BINARY_PATH}"
DESKTOP_BINARY_PATH="${DEFAULT_DESKTOP_BINARY_PATH}"
API_KEY="${MANIFEED_WORKER_API_KEY:-}"

log() {
  printf '[installer][rss] %s\n' "$*"
}

die() {
  printf '[installer][rss][error] %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Usage: install.sh [options]

Options:
  --gui                      Force GUI mode with zenity.
  --cli                      Force terminal mode.
  --non-interactive          Require values through flags/env vars.
  --binary PATH              Path to the worker-rss binary.
  --desktop-binary PATH      Path to the shared desktop binary.
  --api-key TOKEN            Worker API key.
  --install-service          Also install a systemd --user service.
  --help                     Show this help.
EOF
}

have_cmd() {
  command -v "$1" >/dev/null 2>&1
}

maybe_enable_gui() {
  if [ "$UI_MODE" = "cli" ]; then
    return 1
  fi
  if [ "$UI_MODE" = "gui" ]; then
    have_cmd zenity || die "GUI mode requested but zenity is not installed"
    return 0
  fi
  if [ -n "${DISPLAY:-}" ] && have_cmd zenity; then
    return 0
  fi
  return 1
}

prompt_cli() {
  if [ -z "$API_KEY" ]; then
    read -r -s -p "Worker API key: " API_KEY
    printf '\n'
  fi
}

prompt_gui() {
  API_KEY=$(zenity --password \
    --title="Manifeed Workers Installer" \
    --text="Worker API key") || exit 1
}

parse_args() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --gui)
        UI_MODE="gui"
        ;;
      --cli)
        UI_MODE="cli"
        ;;
      --non-interactive)
        NON_INTERACTIVE=1
        ;;
      --binary)
        shift
        WORKER_BINARY_PATH=${1:-}
        ;;
      --desktop-binary)
        shift
        DESKTOP_BINARY_PATH=${1:-}
        ;;
      --api-key)
        shift
        API_KEY=${1:-}
        ;;
      --install-service)
        INSTALL_SERVICE=1
        ;;
      --help|-h)
        usage
        exit 0
        ;;
      *)
        die "unknown option: $1"
        ;;
    esac
    shift
  done
}

resolve_binary_defaults() {
  if [ "$WORKER_BINARY_PATH" = "$DEFAULT_WORKER_BINARY_PATH" ] && [ ! -x "$WORKER_BINARY_PATH" ] && [ -x "$FALLBACK_WORKER_BINARY_PATH" ]; then
    WORKER_BINARY_PATH="$FALLBACK_WORKER_BINARY_PATH"
  fi
  if [ "$DESKTOP_BINARY_PATH" = "$DEFAULT_DESKTOP_BINARY_PATH" ] && [ ! -x "$DESKTOP_BINARY_PATH" ] && [ -x "$FALLBACK_DESKTOP_BINARY_PATH" ]; then
    DESKTOP_BINARY_PATH="$FALLBACK_DESKTOP_BINARY_PATH"
  fi
  if [ "$ICON_SOURCE_PATH" = "${SCRIPT_DIR}/manifeed-workers.svg" ] && [ ! -f "$ICON_SOURCE_PATH" ] && [ -f "$FALLBACK_ICON_SOURCE_PATH" ]; then
    ICON_SOURCE_PATH="$FALLBACK_ICON_SOURCE_PATH"
  fi
}

write_cli_launcher() {
  local target_path=$1 wrapper_path=$2
  mkdir -p "$(dirname "$wrapper_path")"
  cat >"$wrapper_path" <<EOF
#!/usr/bin/env bash
set -euo pipefail
exec "${target_path}" "\$@"
EOF
  chmod +x "$wrapper_path"
}

install_desktop_launcher() {
  local desktop_file="${APPLICATIONS_DIR}/manifeed-workers.desktop"
  local icon_target="${ICONS_DIR}/manifeed-workers.svg"
  mkdir -p "$APPLICATIONS_DIR" "$ICONS_DIR"
  install -m 0644 "$ICON_SOURCE_PATH" "$icon_target"
  cat >"$desktop_file" <<EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=Manifeed Workers
Comment=Control and monitor Manifeed RSS and embedding workers
Exec=${DESKTOP_INSTALL_DIR}/manifeed-workers
Icon=manifeed-workers
Terminal=false
Categories=Utility;Network;
StartupNotify=true
EOF
}

show_summary_gui() {
  zenity --info \
    --title="Manifeed Workers Installer" \
    --width=520 \
    --text="Installation terminee.

Worker: RSS
Config: ${CONFIG_PATH}
Install dir: ${WORKER_INSTALL_DIR}"
}

main() {
  parse_args "$@"
  resolve_binary_defaults

  [ -x "$WORKER_BINARY_PATH" ] || die "worker binary not found or not executable: ${WORKER_BINARY_PATH}"
  [ -x "$DESKTOP_BINARY_PATH" ] || die "desktop binary not found or not executable: ${DESKTOP_BINARY_PATH}"
  [ -f "$ICON_SOURCE_PATH" ] || die "desktop icon not found: ${ICON_SOURCE_PATH}"

  if [ "$NON_INTERACTIVE" -eq 1 ]; then
    [ -n "$API_KEY" ] || die "--api-key is required in non-interactive mode"
  else
    if maybe_enable_gui; then
      prompt_gui
    else
      prompt_cli
    fi
  fi

  [ -n "$API_KEY" ] || die "worker API key is required"

  mkdir -p "$WORKER_INSTALL_DIR" "$DESKTOP_INSTALL_DIR" "$BIN_DIR" "$CONFIG_DIR"
  install -m 0755 "$WORKER_BINARY_PATH" "${WORKER_INSTALL_DIR}/${WORKER_BINARY_NAME}"
  install -m 0755 "$DESKTOP_BINARY_PATH" "${DESKTOP_INSTALL_DIR}/manifeed-workers"

  if [ "$INSTALL_SERVICE" -eq 1 ]; then
    "${WORKER_INSTALL_DIR}/${WORKER_BINARY_NAME}" install \
      --config "$CONFIG_PATH" \
      --api-key "$API_KEY" \
      --install-service
  else
    "${WORKER_INSTALL_DIR}/${WORKER_BINARY_NAME}" install \
      --config "$CONFIG_PATH" \
      --api-key "$API_KEY"
  fi

  write_cli_launcher \
    "${WORKER_INSTALL_DIR}/${WORKER_BINARY_NAME}" \
    "${BIN_DIR}/manifeed-worker-rss"
  write_cli_launcher \
    "${DESKTOP_INSTALL_DIR}/manifeed-workers" \
    "${BIN_DIR}/manifeed-workers"
  install_desktop_launcher

  log "Installation terminee"
  log "Configuration: ${CONFIG_PATH}"
  log "Worker CLI: ${BIN_DIR}/manifeed-worker-rss"
  log "Desktop app: ${BIN_DIR}/manifeed-workers"
  if [ "$NON_INTERACTIVE" -eq 0 ] && maybe_enable_gui; then
    show_summary_gui
  fi
}

main "$@"
