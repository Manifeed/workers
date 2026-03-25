#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_WORKERS_DIR=$(CDPATH= cd -- "${SCRIPT_DIR}/../../.." && pwd)
REPO_TARGET_RELEASE_DIR="${REPO_WORKERS_DIR}/target/release"

WORKER_BINARY_NAME="worker-source-embedding"
DESKTOP_BINARY_NAME="manifeed-workers"
WORKER_SERVICE_NAME="manifeed-worker-source-embedding"
WORKER_INSTALL_SUBDIR="embedding"

XDG_CONFIG_HOME_VALUE="${XDG_CONFIG_HOME:-${HOME}/.config}"
XDG_DATA_HOME_VALUE="${XDG_DATA_HOME:-${HOME}/.local/share}"
XDG_CACHE_HOME_VALUE="${XDG_CACHE_HOME:-${HOME}/.cache}"

CONFIG_DIR="${XDG_CONFIG_HOME_VALUE}/manifeed"
CONFIG_PATH="${CONFIG_DIR}/workers.json"
DATA_DIR="${XDG_DATA_HOME_VALUE}/manifeed"
WORKER_INSTALL_DIR="${DATA_DIR}/${WORKER_INSTALL_SUBDIR}"
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

ORT_VERSION="1.24.2"
CPU_X64_URL="https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-x64-${ORT_VERSION}.tgz"
CPU_X64_SHA256="43725474ba5663642e17684717946693850e2005efbd724ac72da278fead25e6"
CUDA_X64_URL="https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-x64-gpu-${ORT_VERSION}.tgz"
CUDA_X64_SHA256="bcb42da041f42192e5579de175f7410313c114740a611e230afe9d79be65cc49"
CPU_ARM64_URL="https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-aarch64-${ORT_VERSION}.tgz"
CPU_ARM64_SHA256="6715b3d19965a2a6981e78ed4ba24f17a8c30d2d26420dbed10aac7ceca0085e"

UI_MODE="auto"
NON_INTERACTIVE=0
INSTALL_SERVICE=0
WORKER_BINARY_PATH="${DEFAULT_WORKER_BINARY_PATH}"
DESKTOP_BINARY_PATH="${DEFAULT_DESKTOP_BINARY_PATH}"
API_KEY="${MANIFEED_WORKER_API_KEY:-}"
RUNTIME_BUNDLE="none"
ORT_URL=""
ORT_SHA256=""

log() {
  printf '[installer][embedding] %s\n' "$*"
}

warn() {
  printf '[installer][embedding][warn] %s\n' "$*" >&2
}

die() {
  printf '[installer][embedding][error] %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Usage: install.sh [options]

Options:
  --gui                      Force GUI mode with zenity.
  --cli                      Force terminal mode.
  --non-interactive          Require values through flags/env vars.
  --binary PATH              Path to the worker-source-embedding binary.
  --desktop-binary PATH      Path to the shared desktop binary.
  --api-key TOKEN            Worker API key.
  --install-service          Also install a systemd --user service.
  --help                     Show this help.
EOF
}

have_cmd() {
  command -v "$1" >/dev/null 2>&1
}

detect_package_manager() {
  if have_cmd apt-get; then
    printf 'apt'
    return
  fi
  if have_cmd dnf; then
    printf 'dnf'
    return
  fi
  if have_cmd pacman; then
    printf 'pacman'
    return
  fi
  if have_cmd zypper; then
    printf 'zypper'
    return
  fi
  printf 'none'
}

run_as_root() {
  if [ "$(id -u)" -eq 0 ]; then
    "$@"
    return
  fi
  if have_cmd sudo; then
    sudo "$@"
    return
  fi
  return 1
}

install_packages() {
  local manager=$1
  shift
  [ "$#" -gt 0 ] || return 0

  case "$manager" in
    apt)
      run_as_root apt-get update
      run_as_root apt-get install -y "$@"
      ;;
    dnf)
      run_as_root dnf install -y "$@"
      ;;
    pacman)
      run_as_root pacman -Sy --noconfirm "$@"
      ;;
    zypper)
      run_as_root zypper --non-interactive install "$@"
      ;;
    *)
      return 1
      ;;
  esac
}

ensure_base_tools() {
  local manager missing=()
  manager=$(detect_package_manager)

  have_cmd curl || missing+=("curl")
  have_cmd tar || missing+=("tar")
  have_cmd python3 || missing+=("python3")
  have_cmd sha256sum || missing+=("sha256sum")

  if [ "${#missing[@]}" -eq 0 ]; then
    return 0
  fi
  if [ "$manager" = "none" ]; then
    die "missing required tools: ${missing[*]}"
  fi

  log "Installation des outils manquants: ${missing[*]}"
  case "$manager" in
    apt|dnf|zypper)
      install_packages "$manager" curl tar python3 ca-certificates coreutils
      ;;
    pacman)
      install_packages "$manager" curl tar python ca-certificates coreutils
      ;;
  esac
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

json_field() {
  local field=$1
  python3 -c '
import json
import sys

field = sys.argv[1]
payload = json.load(sys.stdin)
value = payload.get(field)
if isinstance(value, list):
    print("\n".join(str(item) for item in value))
elif value is None:
    print("")
else:
    print(value)
' "$field"
}

probe_json() {
  "$WORKER_BINARY_PATH" probe --config "$CONFIG_PATH" --acceleration gpu
}

choose_runtime_bundle() {
  local probe=$1 recommended_bundle
  recommended_bundle=$(printf '%s' "$probe" | json_field recommended_runtime_bundle)

  case "$recommended_bundle" in
    cuda12)
      RUNTIME_BUNDLE="cuda12"
      ;;
    webgpu)
      warn "Le runtime WebGPU n'est pas provisionne automatiquement sur Linux; fallback CPU."
      RUNTIME_BUNDLE="none"
      ;;
    none|"")
      RUNTIME_BUNDLE="none"
      ;;
    *)
      warn "Bundle runtime inconnu '${recommended_bundle}', fallback CPU."
      RUNTIME_BUNDLE="none"
      ;;
  esac
}

resolve_runtime_artifact() {
  local arch=$1
  case "${RUNTIME_BUNDLE}:${arch}" in
    cuda12:x86_64)
      ORT_URL="$CUDA_X64_URL"
      ORT_SHA256="$CUDA_X64_SHA256"
      ;;
    none:x86_64)
      ORT_URL="$CPU_X64_URL"
      ORT_SHA256="$CPU_X64_SHA256"
      ;;
    none:aarch64)
      ORT_URL="$CPU_ARM64_URL"
      ORT_SHA256="$CPU_ARM64_SHA256"
      ;;
    *)
      die "no ONNX Runtime artifact configured for bundle=${RUNTIME_BUNDLE} arch=${arch}"
      ;;
  esac
}

download_runtime() {
  local runtime_root=$1 arch=$2 archive extract_dir lib_dir
  resolve_runtime_artifact "$arch"

  archive="${runtime_root}/onnxruntime-${RUNTIME_BUNDLE}-${arch}.tgz"
  extract_dir="${runtime_root}/extract"
  rm -rf "$extract_dir"
  mkdir -p "$runtime_root" "$extract_dir"

  log "Telechargement du runtime ONNX ${RUNTIME_BUNDLE} pour ${arch}"
  curl -L --fail --show-error --output "$archive" "$ORT_URL"
  printf '%s  %s\n' "$ORT_SHA256" "$archive" | sha256sum --check --status || die "downloaded ONNX Runtime archive hash mismatch"

  tar -xzf "$archive" -C "$extract_dir"
  lib_dir=$(find "$extract_dir" -maxdepth 3 -type d -path '*/lib' | head -n 1)
  [ -n "$lib_dir" ] || die "unable to locate extracted ONNX Runtime lib directory"

  mkdir -p "${runtime_root}/lib"
  rm -rf "${runtime_root}/lib"/*
  cp -a "${lib_dir}/." "${runtime_root}/lib/"
  [ -f "${runtime_root}/lib/libonnxruntime.so" ] || die "libonnxruntime.so not found after extraction"
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
  local probe_summary=$1
  zenity --info \
    --title="Manifeed Workers Installer" \
    --width=520 \
    --text="Installation terminee.

Worker: Embedding
Config: ${CONFIG_PATH}
Install dir: ${WORKER_INSTALL_DIR}

Probe:
${probe_summary}"
}

main() {
  parse_args "$@"
  ensure_base_tools
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

  local probe probe_arch runtime_root
  probe=$(probe_json)
  probe_arch=$(printf '%s' "$probe" | json_field arch)
  choose_runtime_bundle "$probe"

  mkdir -p "$WORKER_INSTALL_DIR" "$DESKTOP_INSTALL_DIR" "$BIN_DIR" "$CONFIG_DIR"
  install -m 0755 "$WORKER_BINARY_PATH" "${WORKER_INSTALL_DIR}/${WORKER_BINARY_NAME}"
  install -m 0755 "$DESKTOP_BINARY_PATH" "${DESKTOP_INSTALL_DIR}/manifeed-workers"

  runtime_root="${WORKER_INSTALL_DIR}/runtime"
  download_runtime "$runtime_root" "$probe_arch"

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
    "${BIN_DIR}/manifeed-worker-source-embedding"
  write_cli_launcher \
    "${DESKTOP_INSTALL_DIR}/manifeed-workers" \
    "${BIN_DIR}/manifeed-workers"
  install_desktop_launcher

  log "Installation terminee"
  log "Configuration: ${CONFIG_PATH}"
  log "Worker CLI: ${BIN_DIR}/manifeed-worker-source-embedding"
  log "Desktop app: ${BIN_DIR}/manifeed-workers"
  log "Runtime bundle: ${RUNTIME_BUNDLE}"
  if [ "$NON_INTERACTIVE" -eq 0 ] && maybe_enable_gui; then
    show_summary_gui "$probe"
  fi
}

main "$@"
