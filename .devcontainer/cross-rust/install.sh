#!/bin/bash
set -ex
rustup target add aarch64-unknown-linux-musl
rustup target add x86_64-unknown-linux-musl
rustup target add x86_64-pc-windows-gnu
sudo apt-get update
sudo apt-get install -y --no-install-recommends musl-tools mingw-w64
ARCH=$(uname -m)
VENDOR="unknown"
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
if [ "${ARCH}" == "aarch64" ]; then
    sudo apt-get install -y --no-install-recommends gcc-x86-64-linux-gnu gcc-multilib-x86-64-linux-gnu libc6-dev-amd64-cross
fi
if [ "${ARCH}" == "x86_64" ]; then
    sudo apt-get install -y --no-install-recommends gcc-aarch64-linux-gnu libc6-dev-arm64-cross
fi

# CURRDIR=$(pwd)
# mkdir -p /tmp/testbuild
# cargo new --bin testapp
# cd testapp
# cargo build --target aarch64-unknown-linux-musl
# cargo build --target x86_64-unknown-linux-musl
# cargo build --target x86_64-pc-windows-gnu
# rm -rf /tmp/testbuild
# cd $CURRDIR
bash ./install-from-binstall-release.sh
export BINSTALL_DISABLE_TELEMETRY=true
export TARGETS="--targets $ARCH-$VENDOR-$OS-musl --targets $ARCH-$VENDOR-$OS-gnu"
cargo binstall -y $TARGETS cargo-make
cargo binstall -y $TARGETS cargo-nextest
cargo binstall -y $TARGETS cargo-llvm-cov
