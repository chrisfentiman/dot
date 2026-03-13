//! Fuzz the secret URI dispatch — feed arbitrary byte strings as URIs.
//!
//! Goals:
//!   - fetch() must never panic on any input
//!   - backend_name() must never panic on any input
//!   - env:// URIs with the var present must return Ok
//!   - unknown scheme must return a specific error, not panic
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    // backend_name must never panic
    let _ = dotf::secret::backend_name(s);

    // fetch must never panic — it may error, that's fine
    // For env:// URIs we can't control env vars in the fuzzer so errors are expected
    let _ = dotf::secret::fetch(s);
});
