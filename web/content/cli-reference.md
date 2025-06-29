+++
title = "CLI Reference"
weight = 2
template = "docs/page.html"
+++

# CLI Reference

A detailed reference for the HermitGrab command line interface.

## Core Commands

### `hermitgrab apply`

Applies configurations based on the specified profiles or tags. This is the main command you will use.

```sh
hermitgrab apply [--profile <name>...] [--tags <tags>...]
```

- `--profile, -p`: Specify one or more profiles to activate.
- `--tags, -t`: Specify one or more individual tags to activate.

### `hermitgrab install`

Installs a specific tool defined in any scanned `hermit.toml` file.

```sh
hermitgrab install <name>
```

### `hermitgrab check`

Checks all configurations for errors without applying any changes.

## Profile Management

### `hermitgrab profile list`

Lists all available profiles found in your configurations.

### `hermitgrab profile update`

Adds or removes tags from an existing profile.

```sh
hermitgrab profile update <name> [--add <tags>] [--remove <tags>]
```

## Repository and Configuration Management

### `hermitgrab repo add`

Adds a new Git repository to be managed by HermitGrab.

### `hermitgrab repo update`

Updates all managed Git repositories by pulling the latest changes.

### `hermitgrab new`

Creates a new `hermit.toml` configuration file in the current directory.
