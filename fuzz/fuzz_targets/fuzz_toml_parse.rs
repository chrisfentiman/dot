//! Fuzz TOML deserialization for SecretsFile and SymlinksFile.
//!
//! Goals:
//!   - Parsing arbitrary bytes as SecretsFile/SymlinksFile must never panic
//!   - Valid TOML that passes serde validation must round-trip through
//!     serialize → deserialize without loss
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    // ── SecretsFile: must not panic ──────────────────────────────────────
    let secrets_result: Result<dotf::dotfiles::SecretsFile, _> = toml::from_str(s);
    if let Ok(sf) = &secrets_result {
        // Round-trip: serialize back and re-parse — must not lose data.
        let serialized = toml::to_string_pretty(sf).expect("SecretsFile serialize must not fail");
        let reparsed: dotf::dotfiles::SecretsFile =
            toml::from_str(&serialized).expect("SecretsFile round-trip must not fail");
        assert_eq!(
            sf.secrets, reparsed.secrets,
            "SecretsFile round-trip mismatch"
        );
    }

    // ── SymlinksFile: must not panic ─────────────────────────────────────
    let symlinks_result: Result<dotf::dotfiles::SymlinksFile, _> = toml::from_str(s);
    if let Ok(sl) = &symlinks_result {
        let serialized = toml::to_string_pretty(sl).expect("SymlinksFile serialize must not fail");
        let reparsed: dotf::dotfiles::SymlinksFile =
            toml::from_str(&serialized).expect("SymlinksFile round-trip must not fail");
        assert_eq!(
            sl.symlinks, reparsed.symlinks,
            "SymlinksFile round-trip mismatch"
        );
    }
});
