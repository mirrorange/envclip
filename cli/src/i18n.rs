//! Two languages, picked once per process:
//! `--lang` > `ENVCLIP_LANG` > `LC_ALL` > `LC_MESSAGES` > `LANG`.

use std::sync::OnceLock;

static ZH: OnceLock<bool> = OnceLock::new();

fn is_zh(tag: &str) -> bool {
    tag.trim().to_ascii_lowercase().starts_with("zh")
}

/// Call before printing anything; later calls are ignored.
pub fn init(flag: Option<&str>) {
    let zh = match flag {
        Some(tag) => is_zh(tag),
        None => ["ENVCLIP_LANG", "LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .find(|v| !v.is_empty())
            .is_some_and(|v| is_zh(&v)),
    };
    let _ = ZH.set(zh);
}

pub fn zh() -> bool {
    *ZH.get_or_init(|| false)
}

/// `t!("english {}", "中文 {}", arg)` — both strings are checked by `format!`.
#[macro_export]
macro_rules! t {
    ($en:literal, $zh:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::zh() { format!($zh $(, $arg)*) } else { format!($en $(, $arg)*) }
    };
}
