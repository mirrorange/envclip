use std::io::IsTerminal;

#[derive(Clone, Copy)]
pub struct Paint(bool);

impl Paint {
    fn new(tty: bool) -> Self {
        Paint(tty && std::env::var_os("NO_COLOR").is_none())
    }
    pub fn stdout() -> Self {
        Self::new(std::io::stdout().is_terminal())
    }
    pub fn stderr() -> Self {
        Self::new(std::io::stderr().is_terminal())
    }
    fn wrap(self, code: &str, s: &str) -> String {
        if self.0 {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    pub fn green(self, s: &str) -> String {
        self.wrap("32", s)
    }
    pub fn red(self, s: &str) -> String {
        self.wrap("31", s)
    }
    pub fn dim(self, s: &str) -> String {
        self.wrap("2", s)
    }
    pub fn bold(self, s: &str) -> String {
        self.wrap("1", s)
    }
}

/// POSIX single-quote a string so `eval` reproduces it byte for byte.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub fn human_secs(s: u64) -> String {
    match s {
        s if s >= 86400 && s % 86400 == 0 => format!("{}d", s / 86400),
        s if s >= 3600 => format!("{}h", (s + 1800) / 3600),
        s if s >= 60 => format!("{}m", (s + 30) / 60),
        s => format!("{s}s"),
    }
}

/// `90`, `30s`, `10m`, `1h`, `7d` → seconds
pub fn parse_duration(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num, mul) = match s.char_indices().last()? {
        (i, 's') => (&s[..i], 1),
        (i, 'm') => (&s[..i], 60),
        (i, 'h') => (&s[..i], 3600),
        (i, 'd') => (&s[..i], 86400),
        _ => (s, 1),
    };
    num.parse::<u64>().ok()?.checked_mul(mul)
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(sh_quote("plain"), "'plain'");
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
        assert_eq!(sh_quote(""), "''");
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("90"), Some(90));
        assert_eq!(parse_duration("10m"), Some(600));
        assert_eq!(parse_duration("1h"), Some(3600));
        assert_eq!(parse_duration("7d"), Some(604800));
        assert_eq!(parse_duration("h"), None);
        assert_eq!(parse_duration("-1h"), None);
        assert_eq!(human_secs(3600), "1h");
        assert_eq!(human_secs(604800), "7d");
        assert_eq!(human_secs(600), "10m");
    }
}
