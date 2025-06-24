#!/bin/bash
rustup target add aarch64-unknown-linux-musl
rustup target add x86_64-unknown-linux-musl
rustup target add x86_64-pc-windows-gnu
sudo apt-get update
sudo apt-get install -y --no-install-recommends musl-tools mingw-w64
ARCH=$(uname -m)
if [ "${aarch64}" == "aarch64" ]; then
    sudo apt-get install -y --no-install-recommends gcc-x86-64-linux-gnu gcc-multilib-x86-64-linux-gnu
fi
if [ "${aarch64}" == "x86_64" ]; then
    sudo apt-get install -y --no-install-recommends gcc-aarch64-linux-gnu
fi
bash ./install-from-binstall-release.sh
cargo binstall -y cargo-make
cargo binstall -y cargo-nextest
cargo binstall -y cargo-llvm-cov
