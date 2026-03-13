//! Fuzz the line diff algorithm with arbitrary pairs of text.
//!
//! Goals:
//!   - compute_diff() must never panic on any input
//!   - Equal inputs must produce only context lines (no +/- lines)
//!   - All output lines must be prefixed with "  ", "+ ", or "- "
//!   - Context lines must contain content from one of the inputs
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Split the fuzz input in half: first half = old, second half = new
    let mid = data.len() / 2;
    let (old_bytes, new_bytes) = data.split_at(mid);

    let Ok(old_str) = std::str::from_utf8(old_bytes) else { return; };
    let Ok(new_str) = std::str::from_utf8(new_bytes) else { return; };

    let old_lines: Vec<&str> = old_str.lines().collect();
    let new_lines: Vec<&str> = new_str.lines().collect();

    let result = dot::commands::diff::compute_diff(&old_lines, &new_lines);

    // Invariant 1: equal inputs produce no +/- lines
    if old_str == new_str {
        for line in &result {
            assert!(
                !line.starts_with("+ ") && !line.starts_with("- "),
                "equal inputs produced diff line: {line:?}"
            );
        }
    }

    // Invariant 2: all result lines must have a valid prefix
    for line in &result {
        assert!(
            line.starts_with("  ") || line.starts_with("+ ") || line.starts_with("- "),
            "unexpected diff line format: {line:?}"
        );
    }

    // Invariant 3: context lines must contain content from one of the inputs
    for line in &result {
        if let Some(content) = line.strip_prefix("  ") {
            assert!(
                old_lines.contains(&content) || new_lines.contains(&content),
                "context line not found in either input: {content:?}"
            );
        }
    }
});
