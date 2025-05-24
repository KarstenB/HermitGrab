Here is an implementation plan for "HermitGrab" focused on a Minimum Viable Product (MVP) that can install and configure the fish shell and starship prompt from a GitHub repository. The plan is broken down into clear, actionable steps, starting with the core essentials and building toward the MVP.

---

## HermitGrab MVP Implementation Plan

### 1. Project Setup

- **Initialize Rust Project:**  
  - Create a new Rust CLI project using `cargo new hermitgrab --bin`.
  - Set up project structure for future extensibility (e.g., modules for config, commands, tui, etc.).

- **Dependency Management:**  
  - Add dependencies for CLI parsing (e.g., `clap`), YAML/TOML parsing (`serde`, `serde_yaml`, `serde_toml`), and Git operations (`git2` or shell out to `git`).

---

### 2. Core Functionality

#### 2.1. Command-Line Interface (CLI)

- **Basic CLI Skeleton:**  
  - Implement commands: `init`, `install`, `apply`, `status`.
  - Provide help and usage documentation.

#### 2.2. Configuration File Support

- **Support for `hermit.yaml` or `hermit.toml`:**  
  - Define a minimal schema for specifying:
    - Source files (dotfiles) and their target locations.
    - Link type (soft link, hard link, copy).
    - Tags (optional for MVP, but structure for future use).
    - Install applications for example fish, starship

- **Config File Discovery:**  
  - Recursively search for config files in the dotfiles repo.

---

### 3. GitHub Repository Integration

- **Clone/Update Dotfiles Repo:**  
  - Accept a GitHub repository URL as input.
  - Clone the repo to a local directory (e.g., `~/.hermitgrab/dotfiles`).
  - If already cloned, pull latest changes.

---

### 4. Dotfile Management

- **Link/Copy Dotfiles:**  
  - For each entry in the config file, perform the specified action:
    - Create a symlink, hard link, or copy the file to the target location.
    - Handle already exisisting file by prompting the user.
  - Ensure atomic operations to prevent data loss.

---

### 6. Configuration Application

- **Apply Fish and Starship Configs:**  
  - Use the config file to link/copy the relevant configuration files for fish (`config.fish`) and starship (`starship.toml`) to the appropriate locations (e.g., `~/.config/fish/`, `~/.config/starship.toml`).

---

### 7. User Experience

- **Clear Output and Error Handling:**  
  - Print clear status messages for each operation (clone, link, install).
  - Handle errors gracefully and provide actionable feedback.

- **Documentation:**  
  - Provide a simple README with setup instructions and an example `hermit.yaml`/`hermit.toml`.

---

## MVP Acceptance Criteria

- User can run a single command to:
  - Clone a dotfiles repo from GitHub.
  - Install (or prompt to install) fish and starship.
  - Link/copy fish and starship config files to the correct locations.
- All actions are logged to the terminal.
- No TUI or advanced tagging/detector logic required for MVP.
