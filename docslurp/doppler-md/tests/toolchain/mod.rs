//! The integration tests' switch for a missing toolchain (the DocSlurp pdftotext, pdfregions,
//! pdfpagefacts, python3): without `DOPPLER_REQUIRE_INTEGRATION` a test that needs one prints a
//! `skipped:` line and passes; with it set to `1`, `true` or `on` (any case), it fails instead.
//! CI that builds the toolchain sets it, so a broken build cannot pass as skipped tests. `0`,
//! `false`, `off` or empty mean unset; any other value fails, rather than silently skipping.

/// Whether a missing toolchain fails the test (`DOPPLER_REQUIRE_INTEGRATION`).
pub fn required() -> bool {
    match std::env::var("DOPPLER_REQUIRE_INTEGRATION") {
        Err(std::env::VarError::NotPresent) => false,
        Ok(v) => match v.to_ascii_lowercase().as_str() {
            "1" | "true" | "on" => true,
            "" | "0" | "false" | "off" => false,
            other => panic!("DOPPLER_REQUIRE_INTEGRATION={other:?}: use 1, true or on (0, false or off to skip)"),
        },
        Err(e) => panic!("DOPPLER_REQUIRE_INTEGRATION: {e}"),
    }
}

/// Skips the calling test, which `needs` something missing (it returns after this), or fails it
/// when the toolchain is required.
pub fn skip(tag: &str, needs: &str) {
    assert!(!required(), "{tag}: DOPPLER_REQUIRE_INTEGRATION is set but this test needs {needs}");
    eprintln!("skipped: {tag}: needs {needs}");
}
