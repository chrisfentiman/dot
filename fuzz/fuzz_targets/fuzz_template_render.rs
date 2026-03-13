//! Fuzz template rendering.
//!
//! Goals:
//!   - render_template_str() must never panic on arbitrary template content
//!   - Malformed {{...}} blocks, nested braces, unicode, null bytes — all must
//!     either render or return Err, never panic or abort
//!   - Secret values containing template syntax (e.g. `{{B}}`) must never be
//!     re-interpreted as placeholders (single-pass invariant)
#![no_main]

use libfuzzer_sys::fuzz_target;
use std::collections::HashMap;

// Set env vars once at process start — they persist across all fuzz iterations.
static INIT: std::sync::Once = std::sync::Once::new();

fuzz_target!(|data: &[u8]| {
    INIT.call_once(|| {
        // Secret A's value contains template syntax referencing B.
        // If the renderer re-scans substituted values, this would cause
        // cross-secret injection.
        unsafe {
            std::env::set_var("_FUZZ_VAL_A", "injected:{{B}}");
        }
        unsafe {
            std::env::set_var("_FUZZ_VAL_B", "bbb");
        }
    });

    let Ok(template_content) = std::str::from_utf8(data) else {
        return;
    };

    let secrets = dotf::dotfiles::SecretsFile {
        secrets: HashMap::from([
            ("A".to_string(), "env://_FUZZ_VAL_A".to_string()),
            ("B".to_string(), "env://_FUZZ_VAL_B".to_string()),
        ]),
    };

    // Must not panic — Ok or Err are both fine.
    let result = dotf::dotfiles::render_template_str(template_content, &secrets);

    // If the template is exactly "{{A}}", verify the injection invariant:
    // the output must contain the literal "{{B}}", NOT "bbb".
    if template_content == "{{A}}" {
        if let Ok(rendered) = &result {
            assert_eq!(
                rendered, "injected:{{B}}",
                "single-pass invariant violated: secret A's value was re-interpreted"
            );
        }
    }
});
