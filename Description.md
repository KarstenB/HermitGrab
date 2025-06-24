I have developed a tool that is called HermitGrab. It is a dotfile manager that is written in Rust. The key principle of operation is centered around tags. Configuration files can provide tags, and they can require tags. Profiles allow to specify which tags should be used. Detectors provide machine specific tags. This allows to for example have a base profile that install most files, and a work profile to configure git with your work eMail and signature key, while the personal profile configures your personal eMail and personal key. In addition to supporting soft-, hard-linking and copying files it also supports installing tools.


The core features are:
* Configuration with hermit.toml files
* Installation of tools
* Builtin universal binary installer (ubi)
* Automatic discovery of hermitgrab repositories with GitHub and GitLab topics
* Github Device Login (for easy access on remote machines)
* Single static binary
* Arbitrary directory structure
* Support for patching files

Example hermit.toml for the fish shell:
```toml
# This configuration file provides the fish shell
provides = ["fish"]
# It requires the built-in detected tag os_family and that it is unix and it will not execute when zsh ia active
requires = ["+os_family=unix", "-zsh"]

# This entry creates a soft-link to the fish configuration file, and creates a backup of the exisiting file, but overwrites an existing backup
[[file]]
source = "config.fish"
target = "~/.config/fish/config.fish"
fallback = "BackupOverwrite"

# Fish aliases
# Ripgrep aliases, only installed when the ripgrep tag is active
[[file]]
source = "functions/egrep.fish"
target = "~/.config/fish/functions/egrep.fish"
requires = ["+ripgrep"]

# Patch the VScode settings in a DevContainer with fish as default shell
[[patch]]
type = "JsonMerge"
source = "vscode_settings.json"
target = "~/.vscode-server/data/Machine/settings.json"
requires = ["user=vscode"]

# Install the fish shell with ubi, but only if `command -v fish` returns non-zero
[[install]]
name = "fish"
check_cmd = "command -v fish"
post_install_cmd = "fish --install < {{ hermit_this_dir }}/confirm.txt"
source = "ubi"
requires = ["+arch=aarch64", "+os=linux"]

[install.variables]
exe = "fish"
url = "https://github.com/fish-shell/fish-shell/releases/download/4.0.2/fish-static-aarch64-4.0.2.tar.xz"

# Sources support templating with handlebars
[sources]
apt = """
#!/bin/bash
[ "$(find /var/lib/apt/lists -type f -mmin +720)" ] && sudo apt-get update
sudo apt-get install -y --no-install-recommends {{ name }} {{ version }}
"""
brew = "brew install {{ name }}"

# Profiles are a collection of tags
[profiles]
default = ["fish"]
karsten = ["bashrc", "bat", "eza", "fd", "fish", "ripgrep", "starship"]
work-rust = ["fish", "git", "rust", "starship"]
```

HermitGrab also has a rich command line to add links, update profiles etc. In the future some functionality will become a TUI to make it even more comfortable to get started.
