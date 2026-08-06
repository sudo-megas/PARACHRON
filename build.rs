fn main() {
    stamp_build_date();
    embed_windows_resources();

    // The headless UI tests in `src/ui_tests.rs` locate elements by id, which
    // the Slint compiler only records when debug info is emitted. Cargo sets
    // `DEBUG` from the profile being built, so release binaries stay lean.
    let debug_info = std::env::var("DEBUG").is_ok_and(|value| value == "true");

    let config = slint_build::CompilerConfiguration::new().with_debug_info(debug_info);
    slint_build::compile_with_config("ui/app.slint", config)
        .expect("failed to compile ui/app.slint");
}

/// Put the icon and the application manifest inside the Windows executable
/// (Chron11).
///
/// CORE §7 has said since it was written that the `.ico` feeds the Windows
/// build. Until this milestone nothing referenced it: the window icon and the
/// title-bar mark both point at PNGs, and `build/icons/parachron.ico` was a file
/// no code had ever opened. Without this, acceptance criterion 5 — "shows its
/// own icon in Explorer and the taskbar" — cannot pass, because the icon
/// Explorer draws for an `.exe` is the one compiled into it as a resource, not
/// one the program sets after it starts.
///
/// The manifest is here for the same reason and is the more load-bearing half:
/// DPI awareness, the UTF-8 code page and `asInvoker` are all read by Windows
/// *before* the program's own code runs. See `build/parachron.manifest`.
fn embed_windows_resources() {
    // Named unconditionally so Cargo tracks them on every target. Cargo's
    // default "re-run when any file changed" heuristic is already replaced by
    // the directives in `stamp_build_date`, so a file that is not named here is
    // a file whose edits do not trigger a rebuild.
    println!("cargo:rerun-if-changed=build/icons/parachron.ico");
    println!("cargo:rerun-if-changed=build/parachron.manifest");

    // **`CARGO_CFG_TARGET_OS`, not `cfg!(windows)`.** Inside a build script the
    // `cfg` macros describe the machine running the script — the *host* — and
    // the question here is what is being built — the *target*. They agree on a
    // native build and disagree on every cross-build, and getting it backwards
    // produces an `.exe` with no icon and no manifest from a build that
    // reported success.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    compile_resources();
}

/// The Windows-host half, where `winresource` is actually a dependency.
///
/// `Cargo.toml` declares it under `[target.'cfg(windows)'.build-dependencies]`,
/// and Cargo resolves *build*-dependency target predicates against the host. So
/// this function can only be compiled when the host is Windows, which is why the
/// target check above and this `cfg` are two separate gates rather than one.
#[cfg(windows)]
fn compile_resources() {
    let mut resources = winresource::WindowsResource::new();

    resources.set_icon("build/icons/parachron.ico");
    resources.set_manifest_file("build/parachron.manifest");

    // The properties Explorer shows on the Details tab of an executable's
    // property sheet. `FileVersion` and `ProductVersion` come from
    // `CARGO_PKG_VERSION` without being set here.
    resources.set("ProductName", "Parachron");
    resources.set("FileDescription", "A desktop vault for purchases");
    resources.set("OriginalFilename", "parachron.exe");
    resources.set("CompanyName", "sudo-megas");
    // The one property that is a licence obligation rather than a nicety: on
    // Windows there is no /usr/share/licenses to install into, so the About
    // pane and this field are the only places the terms appear (CORE §1).
    resources.set(
        "LegalCopyright",
        "Copyright (C) 2026 sudo-megas — GNU AGPL-3.0-only",
    );

    resources
        .compile()
        .expect("failed to compile the Windows icon and manifest into the executable");
}

/// The cross-build case: a Windows target from a host that is not Windows.
///
/// CORE §7 permits "cross-build or `windows-latest` runner", and
/// `release.yml` takes the second route — so this path is not the one releases
/// travel. It exists because the alternative is worse than a warning: with
/// `winresource` unavailable, the resource step can only be skipped, and a
/// silently skipped resource step is a green build that fails criterion 5 on a
/// machine nobody here owns.
///
/// So it is loud. Anybody who moves the Windows job to a cross-build will see
/// this line and know that `winresource` has to become an unconditional
/// build-dependency, with `windres` reachable, before the `.exe` is shippable.
#[cfg(not(windows))]
fn compile_resources() {
    println!(
        "cargo:warning=Cross-building for Windows from a non-Windows host: the icon and \
         application manifest were NOT embedded. `winresource` is declared under \
         [target.'cfg(windows)'.build-dependencies], which Cargo resolves against the host. \
         The resulting .exe will show a default icon and will not be DPI-aware."
    );
}

/// Emit `PARACHRON_BUILD_DATE` for the About pane's release row (CORE §4).
///
/// ISO, because that is CORE §3's storage form; `about.rs` renders it as
/// `DD-MM-YYYY` through the same `fmt_date` every other date on screen goes
/// through. Storage and display stay separate concerns even for a date that
/// never touches a file.
///
/// An existing `PARACHRON_BUILD_DATE` in the environment wins. That is the seam
/// Chron11 needs: a source build honestly reports the day it was compiled, and a
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
