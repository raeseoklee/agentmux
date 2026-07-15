fn main() {
    // Delay-load comctl32 on MSVC. tauri-runtime-wry's Windows dialog code
    // carries a static import of comctl32!TaskDialogIndirect, which exists
    // only in Common-Controls v6. The packaged app resolves it fine (its
    // tauri-build manifest declares the v6 dependency), but `cargo test`
    // executables have no manifest, so the loader binds the system default
    // comctl32 v5 and every test binary dies at load with
    // STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139) before a single test runs.
    // (`cargo:rustc-link-arg-tests` cannot fix this: it only applies to
    // integration-test targets, not the lib's unit-test binary.)
    // Delay-loading defers the import until first call: tests never call it,
    // and the app calls it under its v6 activation context.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        println!("cargo:rustc-link-arg=/DELAYLOAD:comctl32.dll");
        println!("cargo:rustc-link-arg=delayimp.lib");
    }

    tauri_build::build();
}
