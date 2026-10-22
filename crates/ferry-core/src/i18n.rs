//! UI strings, shared with the Svelte frontend through src/lib/i18n/en.json.
//! Core owns them so job errors are worded from the same file as the cards.

use std::{collections::HashMap, sync::LazyLock};

static EN: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../src/lib/i18n/en.json"))
        .expect("src/lib/i18n/en.json must be a flat map of strings")
});

/// The English string for `key`; a missing key returns the key itself so the
/// gap is visible in the UI instead of crashing.
pub fn t(key: &str) -> String {
    EN.get(key).cloned().unwrap_or_else(|| key.to_string())
}

/// `t` with `{name}` placeholders filled from `vars`, as `t` does in src/lib/i18n.ts.
pub fn tf(key: &str, vars: &[(&str, &str)]) -> String {
    vars.iter().fold(t(key), |text, (name, value)| {
        text.replace(&format!("{{{name}}}"), value)
    })
}

#[cfg(test)]
mod tests {
    use super::{t, tf};

    #[test]
    fn known_key() {
        assert_eq!(t("tray.quit"), "Quit Ferry");
    }

    #[test]
    fn missing_key_falls_back_to_the_key() {
        assert_eq!(t("nope.missing"), "nope.missing");
    }

    #[test]
    fn placeholders_are_filled_and_unknown_ones_kept() {
        assert_eq!(
            tf("card.aria", &[("name", "a.png"), ("state", "optimised")]),
            "a.png, optimised"
        );
        assert_eq!(tf("card.aria", &[("name", "a.png")]), "a.png, {state}");
    }
}
