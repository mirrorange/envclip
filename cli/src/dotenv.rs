//! Minimal .env parser. Mirrors worker/public/assets/dotenv.js.

use crate::crypto::{is_name, Vars};

pub fn parse(text: &str) -> Result<Vars, String> {
    let s: Vec<char> = text.replace("\r\n", "\n").replace('\r', "\n").chars().collect();
    let (mut i, mut line) = (0usize, 1usize);
    let mut out = Vars::new();
    let eol = |i: &mut usize| {
        while *i < s.len() && s[*i] != '\n' {
            *i += 1;
        }
    };

    while i < s.len() {
        match s[i] {
            '\n' => {
                i += 1;
                line += 1;
                continue;
            }
            ' ' | '\t' => {
                i += 1;
                continue;
            }
            '#' => {
                eol(&mut i);
                continue;
            }
            _ => {}
        }

        let start = line;
        let mut j = i;
        while j < s.len() && s[j] != '=' && s[j] != '\n' {
            j += 1;
        }
        if j >= s.len() || s[j] != '=' {
            return Err(t!("line {}: expected NAME=value", "第 {} 行：应为 NAME=value", start));
        }
        let raw: String = s[i..j].iter().collect();
        let raw = raw.trim();
        let name = raw
            .strip_prefix("export")
            .filter(|r| r.starts_with([' ', '\t']))
            .map_or(raw, str::trim_start);
        if !is_name(name) {
            return Err(t!("line {}: bad name \"{}\"", "第 {} 行：变量名无效 \"{}\"", start, name));
        }
        i = j + 1;
        while i < s.len() && (s[i] == ' ' || s[i] == '\t') {
            i += 1;
        }

        let mut value = String::new();
        if i < s.len() && (s[i] == '\'' || s[i] == '"') {
            let q = s[i];
            i += 1;
            loop {
                let Some(&ch) = s.get(i) else {
                    return Err(t!("line {}: unterminated quote", "第 {} 行：引号未闭合", start));
                };
                i += 1;
                if ch == q {
                    break;
                }
                if ch == '\n' {
                    line += 1;
                }
                let esc = (q == '"' && ch == '\\')
                    .then(|| s.get(i))
                    .flatten()
                    .and_then(|&c| match c {
                        'n' => Some('\n'),
                        'r' => Some('\r'),
                        't' => Some('\t'),
                        '"' => Some('"'),
                        '\\' => Some('\\'),
                        _ => None,
                    });
                match esc {
                    Some(c) => {
                        value.push(c);
                        i += 1;
                    }
                    None => value.push(ch),
                }
            }
            eol(&mut i);
        } else {
            j = i;
            while j < s.len() && s[j] != '\n' {
                j += 1;
            }
            let rest: String = s[i..j].iter().collect();
            let cut = rest.char_indices().find(|&(k, c)| {
                c == '#' && rest[..k].ends_with(|p: char| p.is_whitespace())
            });
            value = cut.map_or(rest.as_str(), |(k, _)| &rest[..k]).trim().to_string();
            i = j;
        }
        out.push((name.to_string(), value));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_web_parser() {
        let text = [
            "# comment",
            "PLAIN=hello world   # trailing",
            "export EXPORTED=1",
            "EMPTY=",
            "HASH=a#b",
            r"SINGLE='keep \n raw'",
            r#"DOUBLE="tab\tnew\nquote\"""#,
            "MULTI=\"line1",
            "line2\"",
            "  SPACED = v  ",
            "",
        ]
        .join("\r\n");
        let got = parse(&text).unwrap();
        let want: Vec<(&str, &str)> = vec![
            ("PLAIN", "hello world"),
            ("EXPORTED", "1"),
            ("EMPTY", ""),
            ("HASH", "a#b"),
            ("SINGLE", r"keep \n raw"),
            ("DOUBLE", "tab\tnew\nquote\""),
            ("MULTI", "line1\nline2"),
            ("SPACED", "v"),
        ];
        let got: Vec<(&str, &str)> = got.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn errors() {
        crate::i18n::init(Some("en"));
        assert!(parse("NOPE").unwrap_err().contains("line 1"));
        assert!(parse("A=\"open").unwrap_err().contains("unterminated"));
        assert!(parse("1A=x").unwrap_err().contains("bad name"));
    }
}
