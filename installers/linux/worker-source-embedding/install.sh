#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_WORKERS_DIR=$(CDPATH= cd -- "${SCRIPT_DIR}/../../.." && pwd)
REPO_TARGET_RELEASE_DIR="${REPO_WORKERS_DIR}/target/release"
APP_NAME="worker-source-embedding"
DESKTOP_APP_NAME="worker-source-embedding-desktop"
SERVICE_NAME="manifeed-worker-source-embedding.service"
DEFAULT_INSTALL_DIR="${HOME}/.local/share/manifeed/worker-source-embedding"
DEFAULT_CONFIG_DIR="${HOME}/.config/manifeed"
DEFAULT_ENV_FILE="${DEFAULT_CONFIG_DIR}/worker-source-embedding.env"
DEFAULT_CACHE_DIR="${HOME}/.cache/manifeed/worker-source-embedding/models"
DEFAULT_STATUS_FILE="${HOME}/.local/state/manifeed/worker-source-embedding/status.json"
DEFAULT_LOG_FILE="${HOME}/.cache/manifeed/worker-source-embedding/worker.log"
DEFAULT_SYSTEMD_DIR="${HOME}/.config/systemd/user"
DEFAULT_BINARY_PATH="${SCRIPT_DIR}/worker-source-embedding"
DEFAULT_DESKTOP_BINARY_PATH="${SCRIPT_DIR}/worker-source-embedding-desktop"
FALLBACK_BINARY_PATH="${REPO_TARGET_RELEASE_DIR}/worker-source-embedding"
FALLBACK_DESKTOP_BINARY_PATH="${REPO_TARGET_RELEASE_DIR}/worker-source-embedding-desktop"
DEFAULT_ICON_PATH="${SCRIPT_DIR}/manifeed-worker-source-embedding.svg"
DEFAULT_APPLICATIONS_DIR="${HOME}/.local/share/applications"
DEFAULT_ICONS_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"
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
BINARY_PATH="${DEFAULT_BINARY_PATH}"
DESKTOP_BINARY_PATH="${DEFAULT_DESKTOP_BINARY_PATH}"
INSTALL_DIR="${DEFAULT_INSTALL_DIR}"
API_URL="${MANIFEED_API_URL:-http://127.0.0.1:8000}"
API_KEY="${MANIFEED_WORKER_API_KEY:-}"
HF_TOKEN="${MANIFEED_EMBEDDING_HF_TOKEN:-${HF_TOKEN:-}}"
BACKEND_OVERRIDE="${MANIFEED_EMBEDDING_EXECUTION_BACKEND:-auto}"

log() {
  printf '[installer] %s\n' "$*"
}

warn() {
  printf '[installer][warn] %s\n' "$*" >&2
}

die() {
  printf '[installer][error] %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Usage: install.sh [options]

Options:
  --gui                      Force GUI mode with zenity.
  --cli                      Force terminal mode.
  --non-interactive          Require all values through flags/env vars.
  --binary PATH              Path to the worker-source-embedding binary.
  --install-dir PATH         Installation root directory.
  --api-url URL              Backend base URL.
  --api-key TOKEN            Worker API key.
  --hf-token TOKEN           Optional Hugging Face token.
  --backend VALUE            auto|cpu|cuda|webgpu
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

  log "Installing missing tools: ${missing[*]}"
  case "$manager" in
    apt)
      install_packages "$manager" curl tar python3 ca-certificates coreutils
      ;;
    dnf)
      install_packages "$manager" curl tar python3 ca-certificates coreutils
      ;;
    pacman)
      install_packages "$manager" curl tar python ca-certificates coreutils
      ;;
    zypper)
      install_packages "$manager" curl tar python3 ca-certificates coreutils
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
  if [ -z "$API_URL" ]; then
    read -r -p "Backend API URL: " API_URL
  fi
  if [ -z "$API_KEY" ]; then
    read -r -s -p "Worker API key: " API_KEY
    printf '\n'
  fi
  if [ -z "$HF_TOKEN" ]; then
    read -r -s -p "Hugging Face token (optional): " HF_TOKEN
    printf '\n'
  fi
}

prompt_gui() {
  API_URL=$(zenity --entry \
    --title="Manifeed Worker Installer" \
    --text="Backend API URL" \
    --entry-text="${API_URL}") || exit 1

  API_KEY=$(zenity --password \
    --title="Manifeed Worker Installer" \
    --text="Worker API key") || exit 1

  HF_TOKEN=$(zenity --password \
    --title="Manifeed Worker Installer" \
    --text="Hugging Face token (optional, cancel to leave empty)") || HF_TOKEN=""
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
        BINARY_PATH=${1:-}
        ;;
      --desktop-binary)
        shift
        DESKTOP_BINARY_PATH=${1:-}
        ;;
      --install-dir)
        shift
        INSTALL_DIR=${1:-}
        ;;
      --api-url)
        shift
        API_URL=${1:-}
        ;;
      --api-key)
        shift
        API_KEY=${1:-}
        ;;
      --hf-token)
        shift
        HF_TOKEN=${1:-}
        ;;
      --backend)
        shift
        BACKEND_OVERRIDE=${1:-}
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

resolve_repo_binary_defaults() {
  if [ "$BINARY_PATH" = "$DEFAULT_BINARY_PATH" ] && [ ! -x "$BINARY_PATH" ] && [ -x "$FALLBACK_BINARY_PATH" ]; then
    log "Using repo build binary: ${FALLBACK_BINARY_PATH}"
    BINARY_PATH="$FALLBACK_BINARY_PATH"
  fi

  if [ "$DESKTOP_BINARY_PATH" = "$DEFAULT_DESKTOP_BINARY_PATH" ] && [ ! -x "$DESKTOP_BINARY_PATH" ] && [ -x "$FALLBACK_DESKTOP_BINARY_PATH" ]; then
    log "Using repo build desktop binary: ${FALLBACK_DESKTOP_BINARY_PATH}"
    DESKTOP_BINARY_PATH="$FALLBACK_DESKTOP_BINARY_PATH"
  fi
}

probe_json() {
  MANIFEED_API_URL="$API_URL" \
    MANIFEED_WORKER_API_KEY="$API_KEY" \
    MANIFEED_EMBEDDING_HF_TOKEN="$HF_TOKEN" \
    MANIFEED_EMBEDDING_EXECUTION_BACKEND="$BACKEND_OVERRIDE" \
    "$BINARY_PATH" probe
}

json_field() {
  local field=$1
  python3 -c '
import json
import sys

field = sys.argv[1]
payload = json.load(sys.stdin)
value = payload[field]
if isinstance(value, list):
    print("\n".join(str(item) for item in value))
elif value is None:
    print("")
else:
    print(value)
' "$field"
}

choose_runtime_bundle() {
  local probe=$1 recommended_bundle recommended_backend
  recommended_bundle=$(printf '%s' "$probe" | json_field recommended_runtime_bundle)
  recommended_backend=$(printf '%s' "$probe" | json_field recommended_backend)

  case "$recommended_bundle" in
    cuda12)
      RUNTIME_BUNDLE="cuda12"
      EXECUTION_BACKEND="$recommended_backend"
      ;;
    none)
      RUNTIME_BUNDLE="none"
      EXECUTION_BACKEND="cpu"
      ;;
    webgpu)
      warn "WebGPU runtime bundle is not provisioned automatically on Linux yet; falling back to CPU."
      RUNTIME_BUNDLE="none"
      EXECUTION_BACKEND="cpu"
      ;;
    *)
      warn "Unknown runtime bundle recommendation '${recommended_bundle}', falling back to CPU."
      RUNTIME_BUNDLE="none"
      EXECUTION_BACKEND="cpu"
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

  log "Downloading ONNX Runtime bundle ${RUNTIME_BUNDLE} for ${arch}"
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

write_env_file() {
  local env_file=$1 runtime_lib=$2
  mkdir -p "$(dirname "$env_file")" "$DEFAULT_CACHE_DIR" "$(dirname "$DEFAULT_STATUS_FILE")" "$(dirname "$DEFAULT_LOG_FILE")"
  cat >"$env_file" <<EOF
MANIFEED_API_URL=${API_URL}
MANIFEED_WORKER_API_KEY=${API_KEY}
MANIFEED_EMBEDDING_HF_TOKEN=${HF_TOKEN}
MANIFEED_EMBEDDING_EXECUTION_BACKEND=${EXECUTION_BACKEND}
MANIFEED_EMBEDDING_ORT_DYLIB_PATH=${runtime_lib}
MANIFEED_EMBEDDING_CACHE_DIR=${DEFAULT_CACHE_DIR}
MANIFEED_EMBEDDING_STATUS_FILE=${DEFAULT_STATUS_FILE}
EOF
  chmod 600 "$env_file"
}

write_launcher() {
  local launcher_path=$1 env_file=$2
  cat >"$launcher_path" <<EOF
#!/usr/bin/env bash
set -euo pipefail
set -a
source "${env_file}"
set +a
exec "${INSTALL_DIR}/worker-source-embedding" run
EOF
  chmod +x "$launcher_path"
}

install_desktop_launcher() {
  local desktop_file="${DEFAULT_APPLICATIONS_DIR}/manifeed-worker-source-embedding.desktop"
  local icon_target="${DEFAULT_ICONS_DIR}/manifeed-worker-source-embedding.svg"
  mkdir -p "$DEFAULT_APPLICATIONS_DIR" "$DEFAULT_ICONS_DIR"
  install -m 0644 "$DEFAULT_ICON_PATH" "$icon_target"
  cat >"$desktop_file" <<EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=Manifeed Embedding Worker
Comment=Control and monitor the Manifeed embedding worker
Exec=${INSTALL_DIR}/worker-source-embedding-desktop
Icon=manifeed-worker-source-embedding
Terminal=false
Categories=Utility;Network;
StartupNotify=true
EOF
}

write_systemd_service() {
  local service_path=$1 env_file=$2
  mkdir -p "$(dirname "$service_path")"
  cat >"$service_path" <<EOF
[Unit]
Description=Manifeed Source Embedding Worker
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
EnvironmentFile=${env_file}
ExecStart=${INSTALL_DIR}/worker-source-embedding run
Restart=always
RestartSec=5
WorkingDirectory=${INSTALL_DIR}

[Install]
WantedBy=default.target
EOF
}

attempt_systemd_install() {
  local env_file=$1 service_path="${DEFAULT_SYSTEMD_DIR}/${SERVICE_NAME}"
  if ! have_cmd systemctl; then
    warn "systemctl not found; skipping user service installation"
    return 1
  fi

  write_systemd_service "$service_path" "$env_file"

  if systemctl --user daemon-reload >/dev/null 2>&1 \
    && systemctl --user enable --now "$SERVICE_NAME" >/dev/null 2>&1; then
    log "Installed and started ${SERVICE_NAME}"
    return 0
  fi

  warn "unable to enable/start systemd user service automatically; you can start it later with: systemctl --user enable --now ${SERVICE_NAME}"
  return 1
}

show_summary_gui() {
  local probe=$1
  zenity --info \
    --title="Manifeed Worker Installer" \
    --width=520 \
    --text="Installation complete.

Backend API: ${API_URL}
Installed backend: ${EXECUTION_BACKEND}
Runtime bundle: ${RUNTIME_BUNDLE}
Install dir: ${INSTALL_DIR}

Probe summary:
${probe}"
}

main() {
  parse_args "$@"
  ensure_base_tools
  resolve_repo_binary_defaults

  if [ ! -x "$BINARY_PATH" ]; then
    die "worker binary not found or not executable: ${BINARY_PATH}"
  fi
  if [ ! -x "$DESKTOP_BINARY_PATH" ]; then
    die "desktop binary not found or not executable: ${DESKTOP_BINARY_PATH}"
  fi
  if [ ! -f "$DEFAULT_ICON_PATH" ]; then
    die "desktop icon not found: ${DEFAULT_ICON_PATH}"
  fi

  if [ "$NON_INTERACTIVE" -eq 1 ]; then
    [ -n "$API_URL" ] || die "--api-url is required in non-interactive mode"
    [ -n "$API_KEY" ] || die "--api-key is required in non-interactive mode"
  else
    if maybe_enable_gui; then
      prompt_gui
    else
      prompt_cli
    fi
  fi

  [ -n "$API_URL" ] || die "backend API URL is required"
  [ -n "$API_KEY" ] || die "worker API key is required"
  case "$INSTALL_DIR" in
    *" "*)
      die "install directory must not contain spaces: ${INSTALL_DIR}"
      ;;
  esac

  local probe probe_arch runtime_root runtime_lib env_file launcher_path
  probe=$(probe_json)
  probe_arch=$(printf '%s' "$probe" | json_field arch)
  choose_runtime_bundle "$probe"

  log "Probe recommended backend: ${EXECUTION_BACKEND}"
  runtime_root="${INSTALL_DIR}/runtime"
  mkdir -p "$INSTALL_DIR" "${HOME}/.local/bin"
  install -m 0755 "$BINARY_PATH" "${INSTALL_DIR}/worker-source-embedding"
  install -m 0755 "$DESKTOP_BINARY_PATH" "${INSTALL_DIR}/worker-source-embedding-desktop"

  download_runtime "$runtime_root" "$probe_arch"
  runtime_lib="${runtime_root}/lib/libonnxruntime.so"

  env_file="${DEFAULT_ENV_FILE}"
  write_env_file "$env_file" "$runtime_lib"
  install_desktop_launcher

  launcher_path="${HOME}/.local/bin/manifeed-worker-source-embedding"
  write_launcher "$launcher_path" "$env_file"
  if [ "$INSTALL_SERVICE" -eq 1 ]; then
    attempt_systemd_install "$env_file" || true
  fi

  log "Installation complete"
  log "Launcher: ${launcher_path}"
  log "Desktop app: ${INSTALL_DIR}/worker-source-embedding-desktop"
  log "Desktop entry: ${DEFAULT_APPLICATIONS_DIR}/manifeed-worker-source-embedding.desktop"
  log "Env file: ${env_file}"
  log "Run manually: ${launcher_path}"

  if [ "$NON_INTERACTIVE" -eq 0 ] && maybe_enable_gui; then
    show_summary_gui "$probe"
  fi
}

main "$@"
