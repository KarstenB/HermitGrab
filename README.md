<!--
SPDX-FileCopyrightText: 2025 Karsten Becker

SPDX-License-Identifier: GPL-3.0-only
-->

# HermitGrab

A powerful, tag-based dotfile manager written in Rust. Effortlessly manage configurations, install tools, and sync your entire development environment across any machine.

## Features

- **Tag-Based System:** Use tags to conditionally apply configurations. Set up profiles for `work`, `personal`, or specific machines with ease.
- **Tool Installation:** Go beyond dotfiles. Install your favorite CLIs and applications using the same declarative `hermit.toml` file.
- **Universal Binary Installer:** Leverage the built-in `ubi` to fetch and install binaries from URLs, ensuring cross-platform consistency.
- **Automatic Discovery:** Find and use community-managed dotfile repos effortlessly via GitHub and GitLab topics.
- **File Patching:** Atomically patch existing files. Perfect for modifying `settings.json` in VSCode or other JSON configs.
- **Single Static Binary:** Written in Rust for speed and reliability. No dependencies, no runtime, no hassle. Just one binary.

## Example `hermit.toml`

```toml
# This configuration file provides the fish shell
# It requires a unix-like OS and will not run if zsh is a tag
requires = ["fish", "+os_family=unix", "-zsh"]

[[link]]
source = "config.fish"
target = "~/.config/fish/config.fish"
fallback = "BackupOverwrite"

# Conditionally install aliases only when 'ripgrep' tag is active
[[link]]
source = "functions/egrep.fish"
target = "~/.config/fish/functions/egrep.fish"
requires = ["+ripgrep"]

# Patch VSCode settings in a DevContainer
[[patch]]
type = "JsonMerge"
source = "vscode_settings.json"
target = "~/.vscode-server/data/Machine/settings.json"
requires = ["user=vscode"]

# Install the fish shell
[[install]]
name = "fish"
# But only if not already installed
check = "command -v fish"
# Reference a snippet called ubi (universal binary installer)
install = """{{ snippet ubi }}
if [ ! -f "/usr/local/bin/fish" ]; then
    sudo cp $HOME/.local/bin/fish /usr/local/bin/fish
fi
fish --install < {{ dir.this_dir }}/confirm.txt
"""
# Only on Linus
requires = ["+os=linux"]
# The ubi snippet uses variables to install from a URL, which is preprocessed to contain the arch_alias (arm64/amd64)
variables = { exe = "fish", url = "https://github.com/fish-shell/fish-shell/releases/download/4.0.2/fish-static-{{ tag.arch_alias }}-4.0.2.tar.xz" }

[[install]]
name = "fish"
check = "command -v fish"
# On MacOs we simply use brew
install = "brew install fish"
requires = ["+os=macos"]

# Profiles are a simply a named collection of tags
[profiles]
default = ["fish"]
personal = ["fish", "personal"]
work = ["fish", "work"]

# Detectors can automatically enable tags
[detectors]
has_git = { enable_if = "command -v git" }

# Customize your settings for different profiles
[[install]]
name = "Git Personal Email"
check = "[ $(git config --global --get user.email) = \"personal@icloud.com\" ]"
install = """#!/bin/bash
git config --global user.name \"Definitly Myname\"
git config --global user.email \"personal@icloud.com\"
git config --global user.signingkey \"ssh-ed25519 AAAAC3...\"
"""
requires = ["+personal", "+has_git"]

[[install]]
name = "Git Work Email"
check = "[ $(git config --global --get user.email) = \"me@work.com\" ]"
install = """#!/bin/bash
git config --global user.name \"Definitly Myname\"
git config --global user.email \"me@work.com\"
git config --global user.signingkey \"ssh-ed25519 AAAAC3...\"
"""
requires = ["+work", "+has_git"]
```

## Installation

### Install from your domain (Recommended)

```sh
bash -c "$(curl -fsSL https://hermitgrab.app/install.sh)"
```

### Or, build from source with Cargo

```sh
cargo install --git https://github.com/KarstenB/hermitgrab.git hermitgrab
```

## License

Released under the GPL-3.0 License.

Copyright © 2025 - Karsten Becker
