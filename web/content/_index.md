+++
title = "HermitGrab - Modern Dotfile Management"
weight = 0
template = "docs/page.html"
+++

# HermitGrab - Modern Dotfile Management

A powerful, tag-based dotfile manager written in Rust. Effortlessly manage configurations, install tools, and sync your entire development environment across any machine.


<!-- Features Section -->
<section id="features" class="py-20 bg-black/30">
  <div class="container mx-auto px-6">
    <div class="text-center mb-12">
      <h2 class="text-4xl font-bold tracking-tight">One Tool to Rule Them All</h2>
      <p class="mt-3 text-lg text-slate-400">HermitGrab is more than just a symlinker. It's a complete environment manager.</p>
    </div>
    <div class="grid md:grid-cols-2 lg:grid-cols-3 gap-8">
      {% featurecard(title="Tag-Based System", icon="tags") %}
        Use tags to conditionally apply configurations. Set up profiles for <code>work</code>, <code>personal</code>, or specific machines with ease.
      {% end %}
      {% featurecard(title="Tool Installation", icon="terminal-square") %}
        Go beyond dotfiles. Install your favorite CLIs and applications using the same declarative <code>hermit.toml</code> file.
      {% end %}
      {% featurecard(title="Universal Binary Installer", icon="package-check") %}
        Leverage the built-in <code>ubi</code> to fetch and install binaries from URLs, ensuring cross-platform consistency.
      {% end %}
      {% featurecard(title="Automatic Discovery", icon="compass") %}
        Find and use community-managed dotfile repos effortlessly via GitHub and GitLab topics.
      {% end %}
      {% featurecard(title="File Patching", icon="file-json-2") %}
        Atomically patch existing files. Perfect for modifying `settings.json` in VSCode or other JSON configs.
      {% end %}
      {% featurecard(title="Single Static Binary", icon="gem") %}
        Written in Rust for speed and reliability. No dependencies, no runtime, no hassle. Just one binary.
      {% end %}
    </div>
  </div>
</section>

## Simple, Powerful Configuration

Define your entire environment in a single, intuitive TOML file:

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

# Install the fish shell with the universal binary installer (ubi)
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

## Powerful CLI & A Bright Future

A rich command-line interface for power users, with an intuitive TUI on the horizon.

```sh
$ hermitgrab apply --profile work-rust
> Activating profile: work-rust
> ✓ Tag 'fish' provided by '.../fish/hermit.toml'
> ✓ Tag 'git' provided by '.../git/hermit.toml'
> ✓ Tag 'rust' provided by '.../rust/hermit.toml'
> ✓ Tag 'starship' provided by '.../starship/hermit.toml'
> Linking files...
> Done.

$ hermitgrab install eza
> Installing 'eza'...

$ hermitgrab profile update karsten --add nvim
> Profile 'karsten' updated.
```

## Get HermitGrab Now

It's open source and ready to manage your world. Grab the single binary and get started in seconds.

**Install from your domain (Recommended):**

```sh
bash -c "$(curl -fsSL https://hermitgrab.app/install.sh)"
```

**Or, build from source with Cargo:**

```sh
cargo install --git https://github.com/KarstenB/hermitgrab.git hermitgrab
```
