fn main() {
    stamp_build_date();

    // The headless UI tests in `src/ui_tests.rs` locate elements by id, which
    // the Slint compiler only records when debug info is emitted. Cargo sets
    // `DEBUG` from the profile being built, so release binaries stay lean.
    let debug_info = std::env::var("DEBUG").is_ok_and(|value| value == "true");

    let config = slint_build::CompilerConfiguration::new().with_debug_info(debug_info);
    slint_build::compile_with_config("ui/app.slint", config)
        .expect("failed to compile ui/app.slint");
}

/// Emit `PARACHRON_BUILD_DATE` for the About pane's release row (CORE §4).
///
/// ISO, because that is CORE §3's storage form; `about.rs` renders it as
/// `DD-MM-YYYY` through the same `fmt_date` every other date on screen goes
/// through. Storage and display stay separate concerns even for a date that
/// never touches a file.
///
/// An existing `PARACHRON_BUILD_DATE` in the environment wins. That is the seam
/// Chron9 needs: a source build honestly reports the day it was compiled, and a
/// tagged release build has its workflow set the variable to the tag's date, so
/// the asset a user downloads shows the date it was released rather than the
/// date a runner happened to pick it up.
fn stamp_build_date() {
    // Naming the variable is enough to make Cargo re-run this script when it
    // changes. Note that emitting any `rerun-if` directive replaces Cargo's
    // default "re-run when any file in the package changed" heuristic, so the
    // `ui/` directory is named explicitly below — `slint-build` emits its own
    // directives for the files it reads, and this does not depend on that.
    println!("cargo:rerun-if-env-changed=PARACHRON_BUILD_DATE");
    println!("cargo:rerun-if-changed=ui");
    println!("cargo:rerun-if-changed=build.rs");

    let date = std::env::var("PARACHRON_BUILD_DATE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(today_utc);

    println!("cargo:rustc-env=PARACHRON_BUILD_DATE={date}");
}

/// Today, in UTC.
///
/// UTC rather than local: a build date is a fact about a machine somewhere, and
/// the local offset would make two builds of the same commit on two machines
/// disagree by a day for no reason anybody could act on. The app's *user-facing*
/// dates all go through `data::local_offset`, which is a different question.
fn today_utc() -> String {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    time::OffsetDateTime::now_utc()
        .date()
        .format(&format)
        .expect("a date always formats as year-month-day")
}
