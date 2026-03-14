//! Fuzz placeholder name and secret URI validation functions.
//!
//! Goals:
//!   - is_valid_placeholder_name() must never panic on any input
//!   - is_valid_secret_uri() must never panic on any input
//!   - Valid placeholder names must be non-empty ASCII alphanumeric + underscore
//!   - Valid secret URIs must start with a known scheme prefix
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    // ── Placeholder name validation ──────────────────────────────────────
    let name_valid = dotf::dotfiles::is_valid_placeholder_name(s);

    if name_valid {
        // Must be non-empty
        assert!(!s.is_empty(), "empty name accepted as valid");
        // Every char must be ASCII alphanumeric or underscore
        for c in s.chars() {
            assert!(
                c.is_ascii_alphanumeric() || c == '_',
                "invalid char {c:?} in accepted placeholder name {s:?}"
            );
        }
    } else if !s.is_empty() {
        // If rejected and non-empty, must contain at least one invalid char
        let has_invalid = s.chars().any(|c| !c.is_ascii_alphanumeric() && c != '_');
        assert!(
            has_invalid,
            "non-empty name with all valid chars was rejected: {s:?}"
        );
    }

    // ── Secret URI validation ────────────────────────────────────────────
    let uri_valid = dotf::dotfiles::is_valid_secret_uri(s);

    let valid_prefixes = ["pass://", "op://", "bw://", "env://"];
    let has_valid_prefix = valid_prefixes.iter().any(|p| s.starts_with(p));

    if uri_valid {
        assert!(
            has_valid_prefix,
            "URI accepted without valid prefix: {s:?}"
        );
    }
    if has_valid_prefix {
        assert!(
            uri_valid,
            "URI with valid prefix was rejected: {s:?}"
        );
    }
});
