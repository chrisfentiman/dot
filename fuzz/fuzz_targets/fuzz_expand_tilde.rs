//! Fuzz expand_tilde() with arbitrary path strings.
//!
//! Goals:
//!   - Must never panic on any valid UTF-8 input
//!   - Paths starting with "~/" must produce an absolute path under home
//!   - "~" alone must return home dir
//!   - All other inputs must return the input as-is (as a PathBuf)
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    let Ok(result) = dotf::dotfiles::expand_tilde(s) else {
        return;
    };

    // Invariant: tilde paths must be absolute
    if s == "~" || s.starts_with("~/") {
        assert!(
            result.is_absolute(),
            "expand_tilde({s:?}) returned relative path: {result:?}"
        );
    }
});
