#!/bin/sh

# HermitGrab Installation Script
#
# This script downloads and installs the latest or a specified version of HermitGrab.
# It attempts to be POSIX-compliant and should run on most Unix-like systems,
# including Windows via Git Bash, Cygwin, or MSYS.
#
# Options (can be passed as arguments or environment variables):
#
# --prefer-offline / HERMITGRAB_PREFER_OFFLINE=true
#   If a platform-matching binary/archive exists next to this script, use it instead of downloading.
#
# --version=<tag> / HERMITGRAB_VERSION=<tag>
#   Install a specific version tag (e.g., "v0.1.0") instead of the latest.
#
# --installation-path=<path> / HERMITGRAB_INSTALLATION_PATH=<path>
#   Specify a custom directory to install the binary into.
#
# GITHUB_TOKEN
#   A GitHub personal access token to use for API requests, useful for private repos or avoiding rate limits.
#
# Example:
# curl -fsSL https://hermitgrab.app/install.sh | sh
# curl -fsSL https://hermitgrab.app/install.sh | sh -s -- --version=v0.1.0
# GITHUB_TOKEN="ghp_..." HERMITGRAB_VERSION=v0.1.0 sh install.sh

set -e # Exit on any error
set -u # Exit on unset variables

# This function wraps all logic, ensuring the script is fully downloaded before execution.
main() {
  # --- Color definitions for output ---
  C_RESET='\033[0m'
  C_RED='\033[0;31m'
  C_GREEN='\033[0;32m'
  C_YELLOW='\033[0;33m'
  C_CYAN='\033[0;36m'

  # --- Helper functions ---
  info() {
    printf "${C_CYAN}> %s${C_RESET}\n" "$1"
  }

  success() {
    printf "${C_GREEN}✓ %s${C_RESET}\n" "$1"
  }

  warn() {
    printf "${C_YELLOW}! %s${C_RESET}\n" "$1"
  }

  error() {
    printf "${C_RED}✗ %s${C_RESET}\n" "$1" >&2
    exit 1
  }

  _curl() {
    # This function wraps curl to add the Authorization header if GITHUB_TOKEN is set.
    # It calls `command curl` to avoid recursion if this function were aliased.
    if [ -n "${GITHUB_TOKEN:-}" ]; then
      command curl -H "Authorization: token ${GITHUB_TOKEN}" "$@"
    else
      command curl "$@"
    fi
  }

  _wget() {
    # This function wraps wget to add the Authorization header if GITHUB_TOKEN is set.
    if [ -n "${GITHUB_TOKEN:-}" ]; then
      command wget --header="Authorization: token ${GITHUB_TOKEN}" "$@"
    else
      command wget "$@"
    fi
  }


  # --- Parse Arguments and Environment Variables ---
  PREFER_OFFLINE="${HERMITGRAB_PREFER_OFFLINE:-false}"
  REQUESTED_VERSION="${HERMITGRAB_VERSION:-}"
  INSTALLATION_PATH="${HERMITGRAB_INSTALLATION_PATH:-}"

  for arg in "$@"; do
    case "$arg" in
      --prefer-offline) PREFER_OFFLINE=true ;;
      --version=*) REQUESTED_VERSION=$(printf "%s" "$arg" | cut -d'=' -f2) ;;
      --installation-path=*) INSTALLATION_PATH=$(printf "%s" "$arg" | cut -d'=' -f2) ;;
      *) error "Unknown argument: $arg" ;;
    esac
  done

  # --- Acknowledge GITHUB_TOKEN if used ---
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    info "Using GITHUB_TOKEN for API access."
  fi

  # --- Detect Platform and Capabilities ---
  info "Detecting platform and capabilities..."
  OS_TYPE=$(uname -s)
  ARCH_TYPE=$(uname -m)
  FINAL_BINARY_NAME="hermitgrab" # Default final name of the executable
  OS_NAME=""                     # For constructing asset name
  USE_COMPRESSED=false
  EXT=""

  case "$OS_TYPE" in
    Linux)
      OS_NAME="linux"
      if command -v tar >/dev/null; then
        info "Found 'tar', will use compressed archive."
        USE_COMPRESSED=true
        EXT=".tar.gz"
      fi
      ;;
    Darwin)
      OS_NAME="macos"
      if command -v tar >/dev/null; then
        info "Found 'tar', will use compressed archive."
        USE_COMPRESSED=true
        EXT=".tar.gz"
      fi
      ;;
    CYGWIN*|MINGW*|MSYS*)
      OS_NAME="windows"
      FINAL_BINARY_NAME="hermitgrab.exe"
      USE_COMPRESSED=true # Windows is always a zip
      EXT=".zip"
      if ! command -v unzip >/dev/null; then
        error "This script requires 'unzip' on Windows. Please install it to proceed."
      fi
      ;;
    *)
      error "Unsupported operating system: $OS_TYPE"
      ;;
  esac

  case "$ARCH_TYPE" in
    x86_64 | amd64) ARCH="x86_64" ;;
    aarch64 | arm64) ARCH="aarch64" ;;
    *) error "Unsupported architecture: $ARCH_TYPE" ;;
  esac

  ASSET_NAME="hermitgrab-${OS_NAME}-${ARCH}${EXT}"
  UNCOMPRESSED_ASSET_NAME="hermitgrab-${OS_NAME}-${ARCH}"
  success "Platform detected. Asset name: ${ASSET_NAME}"

  # --- Offline Installation Logic ---
  if [ "$PREFER_OFFLINE" = "true" ]; then
    info "Preferring offline installation..."
    # Check for compressed asset first if applicable
    if [ "$USE_COMPRESSED" = "true" ] && [ -f "./${ASSET_NAME}" ]; then
      info "Found local compressed asset: ./${ASSET_NAME}"
      run_extraction "./${ASSET_NAME}" "$FINAL_BINARY_NAME"
      return
    fi
    # Then check for uncompressed binary
    if [ -f "./${UNCOMPRESSED_ASSET_NAME}" ]; then
      info "Found local uncompressed binary: ./${UNCOMPRESSED_ASSET_NAME}"
      run_installation "./${UNCOMPRESSED_ASSET_NAME}" "$FINAL_BINARY_NAME"
      return
    else
      warn "Offline asset/binary not found. Proceeding with download."
    fi
  fi

  # --- Version and Download URL ---
  REPO="KarstenB/hermitgrab"
  if [ -z "$REQUESTED_VERSION" ]; then
    info "Fetching latest version tag from GitHub..."
    # Use command chaining to ensure at least one downloader is available
    LATEST_TAG=$( (command -v curl >/dev/null && _curl -s https://api.github.com/repos/${REPO}/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/') || \
                 (command -v wget >/dev/null && _wget -qO- https://api.github.com/repos/${REPO}/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/') )
    
    if [ -z "$LATEST_TAG" ]; then
      error "Could not fetch latest version tag. Please check your network or specify a version with --version=<tag>."
    fi
    VERSION="$LATEST_TAG"
  else
    VERSION="$REQUESTED_VERSION"
  fi
  success "Using version: $VERSION"

  DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET_NAME}"

  # --- Download Asset ---
  TMP_DIR=$(mktemp -d)
  TMP_ASSET="${TMP_DIR}/${ASSET_NAME}"
  
  info "Downloading from: ${DOWNLOAD_URL}"
  
  if command -v curl >/dev/null; then
    _curl --fail --show-error --location --header 'Accept: application/octet-stream' --output "$TMP_ASSET" "$DOWNLOAD_URL"
  elif command -v wget >/dev/null; then
    _wget --quiet --show-progress --output-document="$TMP_ASSET" "$DOWNLOAD_URL"
  else
    error "Neither curl nor wget is available. Please install one of them to proceed."
  fi
  
  success "Download complete."
  run_extraction "$TMP_ASSET" "$FINAL_BINARY_NAME"
}

# --- This function extracts the binary from an archive ---
run_extraction() {
  local asset_path="$1"
  local final_name="$2"
  local tmp_dir
  tmp_dir=$(dirname "$asset_path")
  local extracted_binary_path=""

  info "Extracting asset..."
  
  case "$asset_path" in
    *.tar.gz)
      tar -xzf "$asset_path" -C "$tmp_dir"
      # Archives for Linux/macOS are assumed to contain a binary named 'hermitgrab'
      extracted_binary_path="${tmp_dir}/hermitgrab"
      ;;
    *.zip)
      # The -o flag overwrites files without prompting, which is good for scripts
      unzip -oq "$asset_path" -d "$tmp_dir"
      # The Windows zip is assumed to contain 'hermitgrab.exe'
      extracted_binary_path="${tmp_dir}/hermitgrab.exe"
      ;;
    *)
      # Not a compressed file, it's the binary itself
      info "Asset is not an archive. Proceeding with installation."
      extracted_binary_path="$asset_path"
      ;;
  esac

  if [ ! -f "$extracted_binary_path" ]; then
    error "Failed to find binary after extraction."
  fi
  
  success "Extraction complete."
  run_installation "$extracted_binary_path" "$final_name"
}

# --- This function handles the final installation of the binary ---
run_installation() {
  local binary_source_path="$1"
  local final_name="$2"

  # --- Determine Installation Path ---
  if [ -z "$INSTALLATION_PATH" ]; then
    if [ "$(id -u)" -eq 0 ]; then
      info "Running as root. Defaulting to system-wide installation."
      INSTALLATION_PATH="/usr/local/bin"
    else
      info "Running as user. Defaulting to user-local installation."
      INSTALLATION_PATH="${HOME}/.local/bin"
    fi
  fi

  info "Installing to: ${INSTALLATION_PATH}"
  
  # Ensure installation directory exists
  if [ ! -d "$INSTALLATION_PATH" ]; then
    info "Installation directory does not exist. Creating: ${INSTALLATION_PATH}"
    mkdir -p "$INSTALLATION_PATH"
  fi

  # --- Install Binary ---
  install -m 755 "$binary_source_path" "${INSTALLATION_PATH}/${final_name}"
  
  # --- Cleanup temp directory if it exists and wasn't a local file ---
  local source_dir
  source_dir=$(dirname "$binary_source_path")
  if [ -d "$source_dir" ] && [ "$(echo "$source_dir" | grep -c 'tmp')" -gt 0 ]; then
      rm -rf "$source_dir"
  fi

  success "${final_name} installed successfully!"

  # --- Verify Installation and PATH ---
  if command -v "$final_name" >/dev/null; then
    INSTALLED_PATH=$(command -v "$final_name")
    success "Binary is available in your PATH at: ${INSTALLED_PATH}"
    "$final_name" --version
  else
    case ":${PATH}:" in
      *:"${INSTALLATION_PATH}":*)
        error "Installation seems to have failed, even though '${INSTALLATION_PATH}' is in your PATH."
        ;;
      *)
        warn "The directory '${INSTALLATION_PATH}' is not in your shell's PATH variable."
        warn "You will need to add it to your shell's configuration file (e.g., ~/.bashrc, ~/.zshrc) to use '${final_name}' directly."
        warn "Example command to add to your shell profile:"
        printf "  ${C_YELLOW}export PATH=\"\$PATH:${INSTALLATION_PATH}\"${C_RESET}\n"
        ;;
    esac
  fi

  info "Enjoy using HermitGrab!"
}

main "$@"
