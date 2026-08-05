fn main() {
    // The headless UI tests in `src/ui_tests.rs` locate elements by id, which
    // the Slint compiler only records when debug info is emitted. Cargo sets
    // `DEBUG` from the profile being built, so release binaries stay lean.
    let debug_info = std::env::var("DEBUG").is_ok_and(|value| value == "true");

    let config = slint_build::CompilerConfiguration::new().with_debug_info(debug_info);
    slint_build::compile_with_config("ui/app.slint", config)
        .expect("failed to compile ui/app.slint");
}
