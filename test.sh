#!/bin/bash
set -e
GIT_ROOT=$(git rev-parse --show-toplevel)
cd "$GIT_ROOT"
cargo build
HG="$GIT_ROOT/target/debug/hermitgrab"
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color
export RUST_LOG=error

function hg_config_equals() {
    local expected="$GIT_ROOT/test_results/$1"
    local actual="${expected/.json/_actual.json}"
    $HG get config --json "$actual"
    if diff "$expected" "$actual"; then
        echo -e "${GREEN}No diff detected in config${NC}"
    else
        echo -e "${RED}File does not exist: $file${NC}"
        exit 3
    fi
}

function hg_exec_json_equals() {
    local expected="$GIT_ROOT/test_results/$1"
    local actual="${expected/.json/_actual.json}"
    shift
    echo -e "${GREEN}Executing command: $*${NC}"
    "$@" --json "$actual" | tee "$TEMP_DIR/output.txt"
    sed -i "s#${TEMP_DIR}#TEMP_DIR#g" "$actual"
    if diff "$expected" "$actual"; then
        echo -e "${GREEN}No diff detected in config${NC}"
    else
        echo -e "${RED}File does not exist: $file${NC}"
        exit 3
    fi
}

function hg_file_exists() {
    local file="$HERMIT_ROOT/$1"
    if [ -f "$file" ]; then
        echo -e "${GREEN}File exists: $file${NC}"
        return 0
    else
        echo -e "${RED}File does not exist: $file${NC}"
        return 1
    fi
}

function file_exists() {
    local file="$1"
    if [ -f "$file" ]; then
        echo -e "${GREEN}File exists: $file${NC}"
        return 0
    else
        echo -e "${RED}File does not exist: $file${NC}"
        return 1
    fi
}

function exec_contains() {
    local content="$1"
    shift
    echo -e "${GREEN}Executing command: $*${NC}"
    "$@" | tee "$TEMP_DIR/output.txt"
    if grep -q "$content" "$TEMP_DIR/output.txt"; then
        echo -e "${GREEN}Command output contains: $content${NC}"
        return 0
    else
        echo -e "${RED}Command output does not contain: $content${NC}"
        return 2
    fi
}

function hg_is_symlinked() {
    local target="$HERMIT_ROOT/$1"
    local link="$HOME/$2"
    if [ -L "$link" ]; then
        if [ "$(readlink "$link")" == "$target" ]; then
            echo -e "${GREEN}Symlink $link points to $target${NC}"
            return 0
        else
            echo -e "${RED}Symlink $link does not point to $target${NC}"
            return 1
        fi
    else
        echo -e "${RED}$link is not a symlink${NC}"
        return 2
    fi
}

TEMP_DIR=$(mktemp -d)
#trap 'rm -rf "$TEMP_DIR"' EXIT
cd "$TEMP_DIR"
echo "Temporary directory created at $TEMP_DIR, calling init"
export HOME="$TEMP_DIR"
HERMIT_ROOT="$HOME/.hermitgrab"
$HG --version
$HG init create
hg_file_exists ".git/HEAD"

echo "Creating test1 config directory"
echo "Test file content" > "$TEMP_DIR/testfile.txt"
$HG add config test1 --provides "test1"
hg_file_exists "test1/hermit.toml"
hg_config_equals "add_config_test1.json"

echo "Adding test file to existing config directory"
$HG add link ~/testfile.txt --config-dir "test1"
hg_file_exists "test1/testfile.txt"
hg_config_equals "add_testfile_link.json"

echo "Adding another file to the same config directory"
echo "Another test file content" > "$TEMP_DIR/anotherfile.txt"
$HG add link ~/anotherfile.txt --config-dir "test1" --fallback "backupoverwrite" -t '~another'
hg_file_exists "test1/anotherfile.txt"
hg_config_equals "add_anotherfile_link.json"

echo "Add a profile with a tag"
$HG add profile testProfile --tag hello --tag test1
hg_file_exists "hermit.toml"
hg_config_equals "add_profile_test1.json"

echo "Listing tags"
exec_contains "test1" "$HG" get tags

echo "Listing profiles"
exec_contains "testprofile" "$HG" get profiles

hg_exec_json_equals failed_apply.json "$HG" apply -y --profile testProfile

hg_exec_json_equals forced_apply.json "$HG" apply -y --profile testProfile --force
file_exists "$HOME/anotherfile.txt.bak"
file_exists "$HOME/testfile.txt.bak"
hg_is_symlinked "test1/anotherfile.txt" "anotherfile.txt"
hg_is_symlinked "test1/testfile.txt" "testfile.txt"
