#!/bin/bash
cargo build
GIT_DIR=$(git rev-parse --show-toplevel)
mkdir "${GIT_DIR}/tests/test_content/test_output"
export HERMITGRAB_DEBUG=1
HOME="${GIT_DIR}/tests/test_content/test_output" ${GIT_DIR}/target/debug/hermitgrab -c "${GIT_DIR}/tests/test_content/hermit.toml" --verbose --confirm apply