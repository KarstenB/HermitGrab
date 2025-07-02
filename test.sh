#!/bin/bash
set -e
GIT_ROOT=$(git rev-parse --show-toplevel)
cd "$GIT_ROOT"
cargo build
HG="$GIT_ROOT/target/debug/hermitgrab"
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

function hg_file_exists() {
    local file="$HOME/.hermitgrab/$1"
    if [ -f "$file" ]; then
        echo -e "${GREEN}File exists: $file${NC}"
        return 0
    else
        echo -e "${RED}File does not exist: $file${NC}"
        return 1
    fi
}

TEMP_DIR=$(mktemp -d)
#trap 'rm -rf "$TEMP_DIR"' EXIT
cd "$TEMP_DIR"
echo "Temporary directory created at $TEMP_DIR, calling init"
export HOME="$TEMP_DIR"
$HG --version
$HG init create
echo "Creating config directory"
echo "Test file content" > "$TEMP_DIR/testfile.txt"
$HG add config test1 --provides "test1"
hg_file_exists "test1/hermit.toml"
echo "Adding test file to existing config directory"
$HG add link ~/testfile.txt --config-dir "test1"
hg_file_exists "test1/testfile.txt"
echo "Adding another file to the same config directory"
echo "Another test file content" > "$TEMP_DIR/anotherfile.txt"
$HG add link ~/anotherfile.txt --config-dir "test1" --fallback "backupoverwrite" -t '~another'
hg_file_exists "test1/anotherfile.txt" 