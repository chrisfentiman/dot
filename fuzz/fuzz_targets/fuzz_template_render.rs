//! Fuzz template rendering.
//!
//! Goals:
//!   - render_template() must never panic on arbitrary template content
//!   - Malformed {{...}} blocks, nested braces, unicode, null bytes — all must
//!     either render or return Err, never panic or abort
#![no_main]

use libfuzzer_sys::fuzz_target;
use std::collections::HashMap;
use tempfile::TempDir;

// Set env vars once at process start — they persist across all fuzz iterations.
static INIT: std::sync::Once = std::sync::Once::new();

fuzz_target!(|data: &[u8]| {
    INIT.call_once(|| {
        unsafe { std::env::set_var("_FUZZ_VAL_A", "aaa"); }
        unsafe { std::env::set_var("_FUZZ_VAL_B", "bbb"); }
    });

    let Ok(template_content) = std::str::from_utf8(data) else {
        return;
    };

    let tmp = TempDir::new().unwrap();
    let tmpl_path = tmp.path().join("fuzz.tmpl");

    if std::fs::write(&tmpl_path, template_content).is_err() {
        return;
    }

    let secrets = dotf::dotfiles::SecretsFile {
        secrets: HashMap::from([
            ("A".to_string(), "env://_FUZZ_VAL_A".to_string()),
            ("B".to_string(), "env://_FUZZ_VAL_B".to_string()),
        ]),
    };

    // Must not panic
    let _ = dotf::dotfiles::render_template(&tmpl_path, &secrets);
});
