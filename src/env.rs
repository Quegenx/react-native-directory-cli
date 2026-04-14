use std::io::{self, IsTerminal};

const AGENT_ENV_VARS: &[&str] = &["CLAUDECODE", "CODEX_CI", "CODEX"];

pub fn is_agent_environment() -> bool {
    AGENT_ENV_VARS.iter().any(|v| std::env::var(v).is_ok())
}

pub fn stdout_is_tty() -> bool {
    io::stdout().is_terminal()
}

pub fn stdin_is_tty() -> bool {
    io::stdin().is_terminal()
}

pub fn should_auto_json() -> bool {
    is_agent_environment() || !stdout_is_tty()
}

pub fn should_skip_prompts() -> bool {
    is_agent_environment() || !stdin_is_tty()
}
