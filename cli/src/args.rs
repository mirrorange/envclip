//! A deliberately small argument parser.

use std::collections::HashMap;

pub struct Spec {
    /// Options taking a value: (long, short).
    pub values: &'static [(&'static str, char)],
    /// Boolean flags: (long, short).
    pub flags: &'static [(&'static str, char)],
}

#[derive(Default)]
pub struct Parsed {
    opts: HashMap<&'static str, Vec<String>>,
    flags: Vec<&'static str>,
    pub pos: Vec<String>,
    /// Everything after a literal `--`, if present.
    pub rest: Option<Vec<String>>,
}

impl Parsed {
    pub fn opt(&self, k: &str) -> Option<&str> {
        self.opts.get(k).and_then(|v| v.last()).map(String::as_str)
    }
    pub fn all(&self, k: &str) -> &[String] {
        self.opts.get(k).map(Vec::as_slice).unwrap_or(&[])
    }
    pub fn flag(&self, k: &str) -> bool {
        self.flags.contains(&k)
    }
}

pub fn parse(args: Vec<String>, spec: &Spec) -> Result<Parsed, String> {
    let mut p = Parsed::default();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--" {
            p.rest = Some(it.collect());
            break;
        }
        let (name, inline) = if let Some(long) = a.strip_prefix("--") {
            match long.split_once('=') {
                Some((n, v)) => (n.to_string(), Some(v.to_string())),
                None => (long.to_string(), None),
            }
        } else if a.len() == 2 && a.starts_with('-') && a != "-" {
            (a[1..].to_string(), None)
        } else {
            p.pos.push(a);
            continue;
        };
        let short = |c: &char| name.len() == 1 && name.starts_with(*c);
        if let Some((long, _)) = spec.values.iter().find(|(l, s)| *l == name || short(s)) {
            let v = match inline {
                Some(v) => v,
                None => it.next().ok_or_else(|| crate::l!("--{} needs a value", "--{} 需要一个值", long))?,
            };
            p.opts.entry(long).or_default().push(v);
        } else if let Some((long, _)) = spec.flags.iter().find(|(l, s)| *l == name || short(s)) {
            p.flags.push(long);
        } else {
            return Err(crate::l!("unknown option `{}`", "未知选项 `{}`", a));
        }
    }
    Ok(p)
}
