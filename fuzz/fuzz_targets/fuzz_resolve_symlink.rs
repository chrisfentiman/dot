//! Fuzz resolve_symlink_target() and the path traversal security boundary.
//!
//! Goals:
//!   - resolve_symlink_target() must never panic on any input
//!   - Local mode must reject absolute paths and paths starting with ~
//!   - Global mode must handle arbitrary strings without panic
#![no_main]

use libfuzzer_sys::fuzz_target;
use std::path::PathBuf;

fuzz_target!(|data: &[u8]| {
    let Ok(target_str) = std::str::from_utf8(data) else {
        return;
    };

    // ── Global mode: must not panic ──────────────────────────────────────
    let global_ctx = dotf::dotfiles::DotfContext::global();
    let _ = global_ctx.resolve_symlink_target(target_str);

    // ── Local mode: must not panic, must reject absolute/tilde paths ─────
    let local_ctx = dotf::dotfiles::DotfContext::local(PathBuf::from("/tmp/fuzz-project"));
    let local_result = local_ctx.resolve_symlink_target(target_str);

    if target_str.starts_with('/') || target_str.starts_with('~') {
        assert!(
            local_result.is_err(),
            "local mode should reject absolute/tilde path: {target_str:?}"
        );
    }
});
