#!/bin/bash
rustup target add aarch64-unknown-linux-musl
sudo apt-get update && sudo apt-get install -y --no-install-recommends musl-tools