#!/bin/bash
set -ex
# Inspired by https://github.com/devcontainers/features/blob/main/src/rust/install.sh
export CARGO_HOME="${CARGO_HOME:-"/usr/local/cargo"}"
export RUSTUP_HOME="${RUSTUP_HOME:-"/usr/local/rustup"}"
USERNAME="${USERNAME:-"${_REMOTE_USER:-"automatic"}"}"
# Determine the appropriate non-root user
if [ "${USERNAME}" = "auto" ] || [ "${USERNAME}" = "automatic" ]; then
    USERNAME=""
    POSSIBLE_USERS=("vscode" "node" "codespace" "$(awk -v val=1000 -F ":" '$3==val{print $1}' /etc/passwd)")
    for CURRENT_USER in "${POSSIBLE_USERS[@]}"; do
        if id -u "${CURRENT_USER}" > /dev/null 2>&1; then
            USERNAME=${CURRENT_USER}
            break
        fi
    done
    if [ "${USERNAME}" = "" ]; then
        USERNAME=root
    fi
elif [ "${USERNAME}" = "none" ] || ! id -u "${USERNAME}" > /dev/null 2>&1; then
    USERNAME=root
fi

rustup target add aarch64-unknown-linux-musl
rustup target add x86_64-unknown-linux-musl
rustup target add x86_64-pc-windows-gnu
apt-get update
apt-get install -y --no-install-recommends musl-tools mingw-w64
ARCH=$(uname -m)
VENDOR="unknown"
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
if [ "${ARCH}" == "aarch64" ]; then
    apt-get install -y --no-install-recommends gcc-x86-64-linux-gnu gcc-multilib-x86-64-linux-gnu libc6-dev-amd64-cross
fi
if [ "${ARCH}" == "x86_64" ]; then
    apt-get install -y --no-install-recommends gcc-aarch64-linux-gnu libc6-dev-arm64-cross
fi
apt-get clean
bash ./install-from-binstall-release.sh
export BINSTALL_DISABLE_TELEMETRY=true
export TARGETS="--targets $ARCH-$VENDOR-$OS-musl --targets $ARCH-$VENDOR-$OS-gnu"
cargo binstall -y $TARGETS cargo-make
cargo binstall -y $TARGETS cargo-nextest
cargo binstall -y $TARGETS cargo-llvm-cov

chown -R "${USERNAME}:rustlang" "${RUSTUP_HOME}" "${CARGO_HOME}"
chmod g+r+w+s "${RUSTUP_HOME}" "${CARGO_HOME}"
