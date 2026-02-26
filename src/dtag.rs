use nostr_sdk::hashes::{sha256, Hash};

/// Compute a deterministic 8-hex-char suffix from a preimage string.
fn suffix(preimage: &str) -> String {
    sha256::Hash::hash(preimage.as_bytes())
        .to_string()
        .chars()
        .take(8)
        .collect()
}

/// Normalize a human string into a URL-safe slug (`[a-z0-9-]`).
///
/// Returns `fallback` if the input normalizes to empty.
pub fn normalize(input: &str, fallback: &str) -> String {
    let slug: String = input
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_whitespace() { '-' } else { c })
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    if slug.is_empty() {
        fallback.to_string()
    } else {
        slug
    }
}

/// Generate a deterministic d-tag for a list header (kind 39998).
///
/// Format: `{slug}--{8-char-hex-suffix}`
pub fn header_dtag(name_singular: &str, pubkey_hex: &str) -> String {
    let slug = normalize(name_singular, "list");
    let sfx = suffix(&format!("header|{pubkey_hex}|{slug}"));
    format!("{slug}--{sfx}")
}

/// Generate a deterministic d-tag for a list item (kind 39999).
///
/// Format: `{slug}--{8-char-hex-suffix}`
///
/// The suffix is derived from the raw `anchor_value` (not the slug) to preserve
/// sensitivity to the original input.
pub fn item_dtag(parent_z: &str, anchor_value: &str) -> String {
    let slug = normalize(anchor_value, "item");
    let sfx = suffix(&format!("item|{parent_z}|{anchor_value}"));
    format!("{slug}--{sfx}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // normalize
    // -----------------------------------------------------------------------

    #[test]
    fn normalize_simple_lowercase() {
        assert_eq!(normalize("Hello World", "x"), "hello-world");
    }

    #[test]
    fn normalize_trims_whitespace() {
        assert_eq!(normalize("  spaced  ", "x"), "spaced");
    }

    #[test]
    fn normalize_collapses_whitespace_runs() {
        assert_eq!(normalize("a   b   c", "x"), "a-b-c");
    }

    #[test]
    fn normalize_strips_special_chars() {
        assert_eq!(
            normalize("AI Agents! On @Nostr?", "x"),
            "ai-agents-on-nostr"
        );
    }

    #[test]
    fn normalize_preserves_digits() {
        assert_eq!(normalize("Web3 Tools 42", "x"), "web3-tools-42");
    }

    #[test]
    fn normalize_strips_unicode() {
        assert_eq!(normalize("café résumé", "x"), "caf-rsum");
    }

    #[test]
    fn normalize_empty_input_returns_fallback() {
        assert_eq!(normalize("", "item"), "item");
    }

    #[test]
    fn normalize_all_special_chars_returns_fallback() {
        assert_eq!(normalize("!@#$%^&*()", "list"), "list");
    }

    #[test]
    fn normalize_only_whitespace_returns_fallback() {
        assert_eq!(normalize("   ", "list"), "list");
    }

    #[test]
    fn normalize_leading_trailing_hyphens_trimmed() {
        assert_eq!(normalize("--hello--", "x"), "hello");
    }

    #[test]
    fn normalize_repeated_hyphens_collapsed() {
        assert_eq!(normalize("a---b", "x"), "a-b");
    }

    #[test]
    fn normalize_numeric_only() {
        assert_eq!(normalize("12345", "x"), "12345");
    }

    // -----------------------------------------------------------------------
    // header_dtag
    // -----------------------------------------------------------------------

    #[test]
    fn header_dtag_format() {
        let result = header_dtag("AI Agents on Nostr", "aabbccdd");
        assert!(result.starts_with("ai-agents-on-nostr--"));
        assert_eq!(result.len(), "ai-agents-on-nostr--".len() + 8);
    }

    #[test]
    fn header_dtag_deterministic() {
        let a = header_dtag("test", "pubkey1");
        let b = header_dtag("test", "pubkey1");
        assert_eq!(a, b);
    }

    #[test]
    fn header_dtag_different_names_differ() {
        let a = header_dtag("alpha", "pubkey1");
        let b = header_dtag("beta", "pubkey1");
        assert_ne!(a, b);
    }

    #[test]
    fn header_dtag_different_pubkeys_differ() {
        let a = header_dtag("test", "pubkey1");
        let b = header_dtag("test", "pubkey2");
        assert_ne!(a, b);
    }

    #[test]
    fn header_dtag_empty_name_uses_fallback() {
        let result = header_dtag("", "pubkey1");
        assert!(result.starts_with("list--"));
    }

    // -----------------------------------------------------------------------
    // item_dtag
    // -----------------------------------------------------------------------

    #[test]
    fn item_dtag_format() {
        let result = item_dtag("39998:pk:my-list", "https://example.com/resource");
        assert!(result.contains("--"));
        let parts: Vec<&str> = result.rsplitn(2, "--").collect();
        assert_eq!(parts[0].len(), 8); // suffix
    }

    #[test]
    fn item_dtag_deterministic() {
        let a = item_dtag("parent-z", "https://example.com");
        let b = item_dtag("parent-z", "https://example.com");
        assert_eq!(a, b);
    }

    #[test]
    fn item_dtag_different_parents_differ() {
        let a = item_dtag("parent-a", "https://example.com");
        let b = item_dtag("parent-b", "https://example.com");
        assert_ne!(a, b);
    }

    #[test]
    fn item_dtag_different_anchors_differ() {
        let a = item_dtag("parent", "https://a.com");
        let b = item_dtag("parent", "https://b.com");
        assert_ne!(a, b);
    }

    #[test]
    fn item_dtag_empty_anchor_uses_fallback() {
        let result = item_dtag("parent-z", "");
        assert!(result.starts_with("item--"));
    }

    // -----------------------------------------------------------------------
    // suffix
    // -----------------------------------------------------------------------

    #[test]
    fn suffix_length_is_8() {
        assert_eq!(suffix("anything").len(), 8);
    }

    #[test]
    fn suffix_is_hex() {
        let s = suffix("test-input");
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn suffix_deterministic() {
        assert_eq!(suffix("same"), suffix("same"));
    }

    #[test]
    fn suffix_different_inputs_differ() {
        assert_ne!(suffix("alpha"), suffix("beta"));
    }
}
