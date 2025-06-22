# HermitGrab

A modern, user-friendly dotfile manager built with Rust. HermitGrab helps you manage, install, and sync your dotfiles and developer tools across multiple machines.

## Automatic Tag Detection

HermitGrab automatically detects and adds the following tags based on your environment:

- **Hostname:** The current machine's hostname (e.g., `desktop-a`, `laptop-b`)
- **Architecture:** The CPU architecture in Docker style (e.g., `amd64`, `arm64`)
- **OS:** The operating system (e.g., `macos`, `ubuntu`, `debian`, `windows`)
- **OS Version:** The numeric OS version (e.g., `24.04`, `12.7.1`)
- **OS Version Nickname:** The OS version codename or nickname (e.g., `bookworm`, `Noble Numbat`, `Sonoma`)

These tags are available for use in your  `hermit.toml` files to conditionally enable or disable configurations and installations.

## MVP Usage

### 1. Clone your dotfiles repository

```sh
hermitgrab init https://github.com/yourusername/your-dotfiles-repo.git
```

### 2. Apply configuration (link/copy dotfiles, install fish & starship)

```sh
hermitgrab apply
```

### 3. Check status

```sh
hermitgrab status
```

## Example `hermit.toml`

```yaml
files:
  - source: fish/config.fish
    target: ~/.config/fish/config.fish
    link: soft
  - source: starship/starship.toml
    target: ~/.config/starship.toml
    link: soft
install:
  - fish
  - starship
```

## Requirements
- Rust (for building HermitGrab)
- git (for cloning dotfiles)

## Roadmap
- TUI mode
- Tagging and detectors
- Smart-patching
- More package manager support
