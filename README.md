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
provides = ["fish"]
# It requires a unix-like OS and will not run if zsh is a tag
requires = ["+os_family=unix", "-zsh"]

# Soft-link the main fish config, with a backup strategy
[[file]]
source = "config.fish"
target = "~/.config/fish/config.fish"
fallback = "BackupOverwrite"

# Conditionally install aliases only when 'ripgrep' tag is active
[[file]]
source = "functions/egrep.fish"
target = "~/.config/fish/functions/egrep.fish"
requires = ["+ripgrep"]

# Patch VSCode settings in a DevContainer
[[patch]]
type = "JsonMerge"
source = "vscode_settings.json"
target = "~/.vscode-server/data/Machine/settings.json"
requires = ["user=vscode"]

[[install]]
name = "fish"
check_cmd = "command -v fish"
source = "ubi"
requires = ["+arch=aarch64", "+os=linux"]

[install.variables]
exe = "fish"
url = "https://..."

# Profiles are collections of tags to activate
[profiles]
default = ["fish"]
karsten = ["bashrc", "bat", "fish", "ripgrep"]
work-rust = ["fish", "git", "rust", "starship"]
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

Copyright © 2024 - The HermitGrab Developers
