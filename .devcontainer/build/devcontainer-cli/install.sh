#!/bin/bash
set -ex
apt-get update
apt-get install -y --no-install-recommends npm
apt-get clean
npm install -g @devcontainers/cli
