### **Product Requirements Document: "HermitGrab" - A Dotfile Manager**

---

### **1. Introduction**

"HermitGrab" is a modern, user-friendly dotfile manager built with Rust. It is a command-line tool that also features an interactive Terminal User Interface (TUI) to simplify the management of dotfiles across multiple machines and environments. HermitGrab's core philosophy is to provide a powerful and flexible solution that is exceptionally easy to get started with. It allows users to maintain their configuration files in any directory structure and use simple configuration files to define how they are managed. It also syncs them with any GIT repositories but has special support got GitHub.

### **2. User Personas**

* **The Beginner Developer:** New to managing dotfiles, they need a tool that is intuitive and requires minimal initial setup. The interactive TUI mode is perfect for them to visually manage their configurations.
* **The Power User:** An experienced developer who manages a complex set of dotfiles across different operating systems, distributions, and machines (work and personal). They require a highly configurable and efficient tool to automate their setup.
* **The System Administrator:** Manages configurations for multiple user accounts or servers. They will benefit from the ability to script and automate the deployment of standardized configurations. On some machines nothing is installed and they quickly want to get up either a basic or fully fledged environment.

---

### **3. Functional Requirements**

#### **3.1. Core Functionality**

* **Command-Line Interface (CLI):** A robust and well-documented CLI will be the primary way to interact with HermitGrab.
* **Interactive TUI Mode:** An optional TUI that provides a visual way to see the status of dotfiles, select tags, and execute commands.
* **Arbitrary File Structure:** Users can organize their dotfiles in any folder structure they prefer. HermitGrab will not impose a specific layout.

#### **3.2. Configuration**

* **Configuration Files:** At any level of the directory structure, a `hermit.yaml` or `hermit.toml` file can be created to define how files and directories within that location should be handled.
* **Link Types:** The configuration file will allow users to specify one of the following actions for each file or directory:
    * **Soft Link:** Creates a symbolic link to the target location.
    * **Hard Link:** Creates a hard link to the target location.
    * **Copy:** Copies the file or directory to the target location.
* **Tagging System:**
    * Users can define **tags** within the `hermit.yaml` or `hermit.toml` files to conditionally enable or disable certain configurations.
    * Example tags include:
        * **By Role:** `work`, `personal`
        * **By OS:** `windows`, `macos`, `linux`
        * **By Distribution:** `ubuntu`, `arch`, `fedora`, `debian`
        * **By Distribution Release:** `24.04`, `bookworm`, `Noble Nunmbat`
        * **By Tool:** `neovim`, `zsh`, `git`, `fish`, `k9s`
        * **By Host:** `desktop-a`, `laptop-b`
        * **Custom:** Any user-defined tag.

#### **3.3. Detectors**

* **Global Detector Configuration:** A global configuration file will allow users to define "detectors" that automatically enable or disable tags based on the environment.
* **Detector Types:** Detectors can be configured to:
    * **Execute Arbitrary Commands:** Run a shell command and check the output or exit code.
    * **Read Environment Variables:** Check the value of an environment variable.
    * **Check for Installed Programs:** Verify if a specific executable is available in the system's `PATH`.
    * **Check File Existence:** Check if a file or directory exists.

#### **3.4. Application Installation**

* HermitGrab will support the installation of applications and tools as part of the dotfile setup process.
* **Supported Installers:**
    * **Python:** `uv` (for `pip` packages)
    * **Rust:** `cargo binstall`, `rustup`
    * **Node.js:** `npm`
    * **Host-Specific Package Managers:** Support for common package managers like `apt`, `pacman`, `brew`, `scoop` etc.

#### **3.5. Smart-patching**
* Allows for some file formats to idempotentially patch values.
* Supported files:
    * **ini-styles** 
    * **TOML**
    * **YAML**
    * **JSON**
    * **LineInFile** an ansible style match and replace

---

### **4. Non-Functional Requirements**

* **Ease of Use:** The tool should be straightforward to install and configure, with a focus on a gentle learning curve for new users. The documentation must be clear, concise, and provide practical examples.
* **Performance:** Being written in Rust, the tool should be fast and have a low memory footprint, especially during file operations and detector execution. It should also be statically linked for portability.
* **Cross-Platform Compatibility:** The tool must be fully functional on major operating systems: Windows, macOS, and Linux.
* **Reliability:** Dotfile operations (linking, copying) must be atomic where possible to prevent data loss or corruption.
* **Extensibility:** The design should allow for future expansion, such as supporting more package managers or version control system integrations.