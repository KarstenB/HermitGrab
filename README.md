# HermitGrab

A modern, user-friendly dotfile manager built with Rust. HermitGrab helps you manage, install, and sync your dotfiles and developer tools across multiple machines.

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

## Example `hermit.yaml`

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
