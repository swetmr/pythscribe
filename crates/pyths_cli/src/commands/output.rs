use owo_colors::OwoColorize;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}

pub fn use_color() -> bool {
    std::env::var("NO_COLOR").is_err()
}

pub fn success(msg: &str) {
    if use_color() {
        println!("{}", msg.green());
    } else {
        println!("{}", msg);
    }
}

pub fn error_summary(msg: &str) {
    if use_color() {
        eprintln!("{}", msg.red().bold());
    } else {
        eprintln!("{}", msg);
    }
}

pub fn warning_summary(msg: &str) {
    if use_color() {
        eprintln!("{}", msg.yellow());
    } else {
        eprintln!("{}", msg);
    }
}

pub fn info(msg: &str, verbosity: Verbosity) {
    if verbosity == Verbosity::Verbose {
        if use_color() {
            eprintln!("{}", msg.dimmed());
        } else {
            eprintln!("{}", msg);
        }
    }
}
