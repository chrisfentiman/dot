//! Fuzz find_dotf_root() with arbitrary directory structures.
//!
//! Goals:
//!   - Must never panic on any valid path
//!   - Must return None or Some(path) where path.join(".dotf").is_dir()
//!   - Result must always be an ancestor of (or equal to) the start path
#![no_main]

use libfuzzer_sys::fuzz_target;
use std::fs;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    // Skip empty or extremely long inputs
    if s.is_empty() || s.len() > 512 {
        return;
    }

    // Create a temp dir structure from the fuzz input.
    // Use the input as a relative path suffix under a temp root.
    let tmp = std::env::temp_dir().join("dotf-fuzz-find-root");
    let _ = fs::remove_dir_all(&tmp);

    // Sanitize: only allow alphanumeric, slash, dot, underscore, hyphen
    let sanitized: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '/' || *c == '.' || *c == '_' || *c == '-')
        .collect();
    if sanitized.is_empty() {
        return;
    }

    let search_dir = tmp.join(&sanitized);
    if fs::create_dir_all(&search_dir).is_err() {
        let _ = fs::remove_dir_all(&tmp);
        return;
    }

    // Test 1: no .dotf anywhere — should not find anything within tmp
    let result = dotf::dotfiles::find_dotf_root(&search_dir);
    if let Some(ref root) = result {
        // If found, it must not be within our tmp dir (since we didn't create .dotf)
        if root.starts_with(&tmp) {
            panic!(
                "found .dotf in tmp without creating one: root={}, search={}",
                root.display(),
                search_dir.display()
            );
        }
    }

    // Test 2: create .dotf at tmp root — should find it
    let dotf_marker = tmp.join(".dotf");
    let _ = fs::create_dir_all(&dotf_marker);
    let result2 = dotf::dotfiles::find_dotf_root(&search_dir);
    if let Some(ref root) = result2 {
        // Result must be an ancestor of search_dir (or equal)
        assert!(
            search_dir.starts_with(root),
            "result {} is not an ancestor of search {}",
            root.display(),
            search_dir.display()
        );
        // .dotf must exist at the result
        assert!(
            root.join(".dotf").is_dir(),
            "result {} does not contain .dotf/",
            root.display()
        );
    }

    let _ = fs::remove_dir_all(&tmp);
});
