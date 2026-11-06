//! Terminal output helpers. Decorations go to stderr; stdout is reserved for
//! machine-consumable output (`env`, `get`, `--json`).

use std::io::{IsTerminal, Write};
use std::sync::OnceLock;
use std::time::Duration;

fn color() -> bool {
    static C: OnceLock<bool> = OnceLock::new();
    *C.get_or_init(|| std::env::var_os("NO_COLOR").is_none() && std::io::stderr().is_terminal())
}

fn paint(code: &str, s: &str) -> String {
    if color() { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() }
}

pub fn green(s: &str) -> String {
    paint("32", s)
}
pub fn dim(s: &str) -> String {
    paint("2", s)
}
pub fn bold(s: &str) -> String {
    paint("1", s)
}
pub fn red(s: &str) -> String {
    paint("31", s)
}
pub fn cyan(s: &str) -> String {
    paint("36", s)
}

pub fn ok(msg: &str) {
    eprintln!("{} {msg}", green("✓"));
}

pub fn info(msg: &str) {
    eprintln!("{} {msg}", dim("·"));
}

pub fn err(msg: &str) {
    eprintln!("{} {msg}", red("✗"));
}

pub fn stdin_tty() -> bool {
    std::io::stdin().is_terminal()
}

/// Reads a line without echo from the controlling terminal.
pub fn hidden(prompt: &str) -> std::io::Result<zeroize::Zeroizing<String>> {
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    rpassword::read_password().map(zeroize::Zeroizing::new)
}

pub fn line(prompt: &str) -> std::io::Result<String> {
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

/// `90s`, `15m`, `12h`, `7d`, or bare seconds.
pub fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    let (num, mul) = match s.chars().last()? {
        's' => (&s[..s.len() - 1], 1),
        'm' => (&s[..s.len() - 1], 60),
        'h' => (&s[..s.len() - 1], 3600),
        'd' => (&s[..s.len() - 1], 86400),
        _ => (s, 1),
    };
    num.parse::<u64>().ok().filter(|n| *n > 0).map(|n| Duration::from_secs(n * mul))
}

pub fn human(d: Duration) -> String {
    let s = d.as_secs();
    let round = |unit: u64| (s + unit / 2) / unit;
    match s {
        0..=119 => format!("{s}s"),
        120..=7199 => format!("{}m", round(60)),
        7200..=172_799 => format!("{}h", round(3600)),
        _ => format!("{}d", round(86400)),
    }
}

/// Single-quotes a string for POSIX shells.
pub fn sh_quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+:@%=".contains(c)) {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(parse_duration("15m"), Some(Duration::from_secs(900)));
        assert_eq!(parse_duration("2d"), Some(Duration::from_secs(172_800)));
        assert_eq!(parse_duration("30"), Some(Duration::from_secs(30)));
        assert_eq!(parse_duration("0h"), None);
        assert_eq!(parse_duration("x"), None);
        assert_eq!(human(Duration::from_secs(86400)), "24h");
        assert_eq!(human(Duration::from_secs(43_199)), "12h");
    }

    #[test]
    fn quoting() {
        assert_eq!(sh_quote("/usr/bin/envclip"), "/usr/bin/envclip");
        assert_eq!(sh_quote("a b"), "'a b'");
        assert_eq!(sh_quote("it's"), "'it'\\''s'");
    }
}
