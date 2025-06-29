use std::io::Write;

use crossterm::style::Stylize;

pub fn hermitgrab_info(msg: &str) {
    println!("{} {}", "[hermitgrab]".bold().cyan(), msg.cyan());
}

pub fn step(msg: &str) {
    println!("{} {}", "      [step]".bold().cyan(), msg.cyan());
}
pub fn choice(msg: &str) {
    println!("{} {}", "    [choice]".bold().blue(), msg.blue());
}

pub fn info(msg: &str) {
    println!("{} {}", "      [info]".bold().cyan(), msg.cyan());
}
pub fn warn(msg: &str) {
    println!("{} {}", "      [warn]".bold().yellow(), msg.yellow());
}
pub fn error(msg: &str) {
    println!("{} {}", "     [error]".bold().red(), msg.red());
}
pub fn success(msg: &str) {
    println!("{} {}", "   [success]".bold().green(), msg.green());
}
pub fn hint(msg: &str) {
    println!("{} {}", "      [hint]".bold().dark_grey(), msg.dark_grey());
}

pub fn stdout(tag: &str, msg: &str) {
    let lines = msg.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return;
    }
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !tag.is_empty() {
            println!(
                "{}[{}] {}",
                "    [stdout]".bold().dark_grey(),
                tag.dark_grey(),
                line.dark_grey()
            );
        } else {
            println!("{} {}", "    [stdout]".bold().dark_grey(), line.dark_grey());
        }
    }
}
pub fn stderr(tag: &str, msg: &str) {
    let lines = msg.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return;
    }
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !tag.is_empty() {
            println!(
                "{}[{}] {}",
                "    [stderr]".bold().dark_red(),
                tag.dark_red(),
                line.dark_red()
            );
        } else {
            println!("{} {}", "    [stderr]".bold().dark_red(), line.dark_red());
        }
    }
}

pub fn prompt(prompt: &str) -> Result<String, std::io::Error> {
    print!("{}", prompt.yellow());
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

#[macro_export]
macro_rules! step {
    ($($arg:tt)*) => {
        $crate::common_cli::step(&format!($($arg)*));
    };
}
#[macro_export]
macro_rules! choice {
    ($($arg:tt)*) => {
        $crate::common_cli::choice(&format!($($arg)*));
    };
}
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::common_cli::info(&format!($($arg)*));
    };
}
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::common_cli::warn(&format!($($arg)*));
    };
}
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::common_cli::error(&format!($($arg)*));
    };
}
#[macro_export]
macro_rules! success {
    ($($arg:tt)*) => {
        $crate::common_cli::success(&format!($($arg)*));
    };
}
#[macro_export]
macro_rules! hermitgrab_info {
    ($($arg:tt)*) => {
        $crate::common_cli::hermitgrab_info(&format!($($arg)*));
    };
}
#[macro_export]
macro_rules! stdout {
    ($tag:tt, $($arg:tt)*) => {
        $crate::common_cli::stdout($tag, &format!($($arg)*));
    };
}
#[macro_export]
macro_rules! stderr {
    ($tag:tt, $($arg:tt)*) => {
        $crate::common_cli::stderr($tag, &format!($($arg)*));
    };
}
#[macro_export]
macro_rules! prompt {
    ($($arg:tt)*) => {
        $crate::common_cli::prompt(&format!($($arg)*))
    };
}
