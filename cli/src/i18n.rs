//! Two languages, zero dependencies: English and 简体中文.
//!
//! The language is picked from `ENVCLIP_LANG`, then `LC_ALL`, `LC_MESSAGES`
//! and `LANG`. Anything starting with `zh` selects Chinese.

use std::sync::OnceLock;

static ZH: OnceLock<bool> = OnceLock::new();

pub fn zh() -> bool {
    *ZH.get_or_init(|| {
        ["ENVCLIP_LANG", "LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .find(|v| !v.is_empty())
            .is_some_and(|v| v.to_ascii_lowercase().starts_with("zh"))
    })
}

/// Pick one of two static strings.
pub fn tr(en: &'static str, zh_: &'static str) -> &'static str {
    if zh() { zh_ } else { en }
}

/// `l!("english {}", "中文 {}", args..)` — a localized `format!`.
#[macro_export]
macro_rules! l {
    ($en:literal, $zh:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::zh() { format!($zh $(, $arg)*) } else { format!($en $(, $arg)*) }
    };
}
