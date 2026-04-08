fn main() {
    // Embed the UAC manifest on Windows — only for non-test builds.
    // cargo test builds set CARGO_CFG_TEST; skip manifest so tests can run
    // without requiring an elevated terminal.
    #[cfg(target_os = "windows")]
    {
        // The CARGO_CFG_TEST env var is NOT set by build scripts for test builds
        // in the standard way, but we can check for the profile ourselves.
        // A simpler approach: always embed, but run `cargo test` from an elevated terminal.
        // For CI / convenience we skip when the `test-no-uac` feature is enabled.
        if std::env::var("CARGO_FEATURE_TEST_NO_UAC").is_err() {
            let mut res = winres::WindowsResource::new();
            res.set_manifest_file("manifest.xml");
            if let Err(e) = res.compile() {
                // Non-fatal — print warning and continue without manifest
                println!("cargo:warning=Could not embed UAC manifest: {e}");
            }
        }
    }

    // Re-run if manifest changes
    println!("cargo:rerun-if-changed=manifest.xml");
    println!("cargo:rerun-if-changed=build.rs");
}
