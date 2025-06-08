use crossterm::style::Stylize;

pub fn hermitgrab_info(msg: &str) {
    println!("{} {}", "[hermitgrab]".bold().cyan(), msg.cyan());
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
pub fn stdout(msg: &str) {
    println!("{} {}", "    [stdout]".bold().dark_grey(), msg.dark_grey());
}
pub fn stderr(msg: &str) {
    println!("{} {}", "    [stderr]".bold().red(), msg.red());
}
