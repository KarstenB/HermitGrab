+++
title = "hermit.toml Configuration"
weight = 1
template = "docs/page.html"
+++

# hermit.toml Configuration

A detailed reference for the `hermit.toml` file format.

## Top-Level Keys

These keys are defined at the root of your `hermit.toml` file.

### `provides = [<tags>]`
A list of tags that this configuration file provides. These tags can be used by other configurations in their `requires` list.

```toml
provides = ["fish", "fish-aliases"]
```

### `requires = [<conditions>]`
A list of conditions that must be met for this configuration file to be processed. If any condition is not met, the entire file is skipped.

- `+tag`: The tag `tag` must be active.
- `-tag`: The tag `tag` must NOT be active.
- `+key=value`: A detected tag `key` must exist and have the value `value`.
- `-key=value`: A detected tag `key` must NOT have the value `value`.

```toml
# Only applies on Unix-like systems where the 'work' profile tag is active
requires = ["+os_family=unix", "+work"]
```

## Sections

Sections define the actions HermitGrab will take. Each section can have its own `requires` key for conditional execution.

### `[[file]]`
Manages linking or copying files and directories.

- `source`: (Required) The path to the source file, relative to the `hermit.toml` file.
- `target`: (Required) The destination path. Tilde expansion (`~`) is supported.
- `method`: (Optional) How to place the file. Can be `Link` (soft link, default), `HardLink`, or `Copy`.
- `fallback`: (Optional) Action to take if the target file already exists. Can be `None` (default, fails), `Backup`, or `BackupOverwrite`.
- `requires`: (Optional) A list of conditions for this specific file action.

```toml
[[file]]
source = "starship.toml"
target = "~/.config/starship.toml"
method = "Link"
fallback = "Backup"
requires = ["+starship"]
```

### `[[patch]]`
Atomically modifies an existing file.

- `source`: (Required) The file containing the patch content.
- `target`: (Required) The destination file to patch.
- `type`: (Required) The patching strategy. Currently, only `JsonMerge` is supported, which performs a deep merge of the source JSON into the target JSON.

```toml
[[patch]]
type = "JsonMerge"
source = "vscode_settings.json"
target = "~/.vscode-server/data/Machine/settings.json"
requires = ["user=vscode"]
```

### `[[install]]`
Installs tools or packages. Can be a script or use the built-in Universal Binary Installer (`ubi`).

- `name`: (Required) A unique name for the installation.
- `source`: (Required) The name of the installer defined in the `[sources]` table (e.g., `apt`, `brew`) or `ubi` for the universal installer.
- `check_cmd`: (Optional) A shell command. If it returns a zero exit code, the installation is skipped. Useful for checking if a tool is already installed.
- `post_install_cmd`: (Optional) A shell command to run after a successful installation.
- `[install.variables]`: A table of key-value pairs to be used in templating for the source script or `ubi` installer.

```toml
# Using a custom source
[[install]]
name = "ripgrep"
source = "apt"
check_cmd = "command -v rg"

# Using the UBI
[[install]]
name = "fish"
source = "ubi"
check_cmd = "command -v fish"
requires = ["+arch=aarch64", "+os=linux"]

[install.variables]
exe = "fish"
url = "https://github.com/fish-shell/fish-shell/releases/download/4.0.2/fish-static-aarch64-4.0.2.tar.xz"
```

### `[sources]`
Defines reusable, templated installer scripts. Uses Handlebars for templating based on variables from the `[[install]]` section.

```toml
[sources]
apt = """
#!/bin/bash
[ \"$(find /var/lib/apt/lists -type f -mmin +720)\" ] && sudo apt-get update
sudo apt-get install -y --no-install-recommends {{ name }}
"""
brew = "brew install {{ name }}"
```

### `[profiles]`
Defines profiles, which are named collections of tags. Running HermitGrab with a profile activates all of its tags.

```toml
[profiles]
default = ["fish"]
work-rust = ["fish", "git", "rust", "starship"]
```
