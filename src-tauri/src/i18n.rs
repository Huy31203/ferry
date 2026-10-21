//! UI strings, shared with the Svelte frontend through src/lib/i18n/en.json.

use std::{collections::HashMap, sync::LazyLock};

static EN: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../src/lib/i18n/en.json"))
        .expect("src/lib/i18n/en.json must be a flat map of strings")
});

/// The English string for `key`; a missing key returns the key itself so the
/// gap is visible in the UI instead of crashing.
pub fn t(key: &str) -> String {
    EN.get(key).cloned().unwrap_or_else(|| key.to_string())
}

#[cfg(test)]
mod tests {
    use super::t;

    #[test]
    fn known_key() {
        assert_eq!(t("tray.quit"), "Quit Ferry");
    }

    #[test]
    fn missing_key_falls_back_to_the_key() {
        assert_eq!(t("nope.missing"), "nope.missing");
    }
}
