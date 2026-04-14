use std::io::{self, IsTerminal, Write};

pub mod hi {
    use owo_colors::OwoColorize;

    pub fn info(s: &str) -> String {
        s.cyan().to_string()
    }

    pub fn dim(s: &str) -> String {
        s.dimmed().to_string()
    }

    pub fn warn(s: &str) -> String {
        s.yellow().to_string()
    }

    pub fn error(s: &str) -> String {
        s.red().to_string()
    }

    pub fn success(s: &str) -> String {
        s.green().to_string()
    }

    pub fn bold(s: &str) -> String {
        s.bold().to_string()
    }
}

pub mod log {
    use owo_colors::OwoColorize;

    pub fn info(msg: impl AsRef<str>) {
        println!("{}", msg.as_ref());
    }

    pub fn dim(msg: impl AsRef<str>) {
        println!("{}", msg.as_ref().dimmed());
    }

    pub fn warn(msg: impl AsRef<str>) {
        eprintln!("{} {}", "⚠".yellow(), msg.as_ref());
    }

    pub fn error(msg: impl AsRef<str>) {
        eprintln!("{} {}", "✗".red().bold(), msg.as_ref());
    }

    pub fn success(msg: impl AsRef<str>) {
        println!("{} {}", "✓".green(), msg.as_ref());
    }

    pub fn r#break() {
        println!();
    }
}

pub fn flush_stdout() {
    let _ = io::stdout().flush();
}

pub fn disable_colors_if_piped() {
    if !io::stdout().is_terminal() {
        owo_colors::set_override(false);
    }
}

#[allow(dead_code)]
pub fn print_header(version: &str) {
    log::dim(format!("rnd v{}", version));
    log::r#break();
}
