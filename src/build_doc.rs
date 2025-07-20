// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use clap::{Command, CommandFactory};

use crate::commands::Cli;

pub fn build_doc() {
    let command = <Cli as CommandFactory>::command();
    print_usage(&command, "hermitgrab", 0, 0);
}

fn print_usage(command: &Command, prefix: &str, base_weight: u64, depth: u64) {
    let mut command = command.clone().bin_name(prefix);
    if command.get_name() == "help" {
        return;
    }
    let file_prefix = prefix.replace(" ", "_");
    let file_name = format!("{file_prefix}.md");
    let styled = command.render_long_help().to_string();
    let no_hermit = prefix.strip_prefix("hermitgrab ").unwrap_or("hermitgrab");
    let weight = base_weight.max(1);
    let md = format!(
        r#"---
title: {no_hermit}
type: docs
weight: {weight}
sidebar:
  open: true
---
```text
{styled}
```
    "#
    );
    std::fs::write(file_name, md).expect("Failed to write");
    let subcommands = command.get_subcommands_mut().collect::<Vec<_>>();
    let increment = match depth {
        0 => 1000,
        1 => 100,
        2 => 10,
        _ => 1,
    };
    for (idx, sub_cmd) in subcommands.into_iter().enumerate() {
        let prefix = format!("{prefix} {}", sub_cmd.get_name());
        let next_weight = base_weight + (idx as u64 + 1) * increment;
        print_usage(sub_cmd, &prefix, next_weight, depth + 1);
    }
}
