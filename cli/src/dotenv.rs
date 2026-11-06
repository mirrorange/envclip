//! Tiny `.env` reader and variable-name rules.

use zeroize::Zeroizing;

pub type Var = (String, Zeroizing<String>);

/// POSIX-portable names: `[A-Za-z_][A-Za-z0-9_]*`.
pub fn valid_name(n: &str) -> bool {
    let mut c = n.chars();
    matches!(c.next(), Some(ch) if ch == '_' || ch.is_ascii_alphabetic())
        && c.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

/// Parses `KEY=value` lines. Supports `export ` prefixes, `#` comments,
/// single quotes (literal), double quotes (`\n \t \" \\` escapes, may span lines).
pub fn parse(src: &str) -> Result<Vec<Var>, String> {
    let mut out: Vec<Var> = Vec::new();
    let mut lines = src.lines().enumerate();
    while let Some((no, line)) = lines.next() {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let t = t.strip_prefix("export ").map(str::trim_start).unwrap_or(t);
        let Some((k, rest)) = t.split_once('=') else {
            return Err(crate::l!("line {}: expected KEY=VALUE", "第 {} 行：应为 KEY=VALUE", no + 1));
        };
        let k = k.trim();
        if !valid_name(k) {
            return Err(crate::l!("line {}: invalid name `{}`", "第 {} 行：无效的变量名 `{}`", no + 1, k));
        }
        let rest = rest.trim_start();
        let v = if let Some(r) = rest.strip_prefix('\'') {
            match r.find('\'') {
                Some(end) => r[..end].to_string(),
                None => return Err(crate::l!("line {}: unterminated quote", "第 {} 行：引号未闭合", no + 1)),
            }
        } else if let Some(r) = rest.strip_prefix('"') {
            let mut buf = String::new();
            let mut cur = r.to_string();
            loop {
                if unescape_until_quote(&cur, &mut buf) {
                    break;
                }
                match lines.next() {
                    Some((_, l)) => {
                        buf.push('\n');
                        cur = l.to_string();
                    }
                    None => {
                        return Err(crate::l!("line {}: unterminated quote", "第 {} 行：引号未闭合", no + 1));
                    }
                }
            }
            buf
        } else {
            // Unquoted: strip trailing ` # comment` and whitespace.
            let v = match rest.find(" #") {
                Some(i) => &rest[..i],
                None => rest,
            };
            v.trim_end().to_string()
        };
        upsert(&mut out, k.to_string(), Zeroizing::new(v));
    }
    Ok(out)
}

/// Appends unescaped chars of `s` to `buf` until an unescaped `"`.
/// Returns `false` if the line ended before the closing quote.
fn unescape_until_quote(s: &str, buf: &mut String) -> bool {
    let mut it = s.chars();
    while let Some(c) = it.next() {
        match c {
            '"' => return true,
            '\\' => match it.next() {
                Some('n') => buf.push('\n'),
                Some('t') => buf.push('\t'),
                Some('r') => buf.push('\r'),
                Some(e) => buf.push(e),
                None => buf.push('\\'),
            },
            c => buf.push(c),
        }
    }
    false
}

pub fn upsert(vars: &mut Vec<Var>, k: String, v: Zeroizing<String>) {
    match vars.iter_mut().find(|(n, _)| *n == k) {
        Some(slot) => slot.1 = v,
        None => vars.push((k, v)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        let src = "# c\nA=1\nexport B = 'x y' \nC=\"l1\\nl2\"\nD=\"multi\nline\"\nE=plain # note\nA=2\n";
        let v = parse(src).unwrap();
        let get = |k: &str| v.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str().to_string());
        assert_eq!(v.len(), 5);
        assert_eq!(get("A").unwrap(), "2");
        assert_eq!(get("B").unwrap(), "x y");
        assert_eq!(get("C").unwrap(), "l1\nl2");
        assert_eq!(get("D").unwrap(), "multi\nline");
        assert_eq!(get("E").unwrap(), "plain");
        assert!(parse("1A=x").is_err());
        assert!(parse("NOEQ").is_err());
    }
}
