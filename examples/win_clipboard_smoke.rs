//! The half of Chron11's criterion 6 that a runner can actually check.
//!
//! Criterion 6 reads: "On Windows, `Add Document` opens a real file dialog and
//! the serial strip really copies. The dependency graph says both backends are
//! there; this criterion is the difference between a graph and a working
//! application." The two halves are not equally testable, and this example
//! exists to keep that asymmetry honest rather than to hide it.
//!
//! **The clipboard is testable without a person.** A Windows runner has a
//! window station; `arboard` either puts a string on the clipboard and reads
//! the same string back, or it does not. That is exactly what the serial strip
//! and the purchase link do (CORE §4: every URL in the app is plain text with
//! copy-to-clipboard), so a passing round-trip here is the same code path
//! passing.
//!
//! **The file dialog is not.** `rfd::AsyncFileDialog::pick_file` shows a modal
//! window and waits for somebody to choose a file. There is no way to answer it
//! from a workflow that would not also make the test meaningless. The Windows
//! spike therefore uploads the built `.exe` as an artefact, and that half of
//! criterion 6 is answered by one person opening the dialog once.
//!
//! Run by `.github/workflows/spike.yml`. It is an example rather than a test so
//! that `cargo test` never depends on a clipboard being present — the headless
//! suite runs on machines that have none, and CORE's harness lesson from Chron9
//! was that a test which needs the environment to cooperate is a test that
//! reports the environment.
//!
//! The string is deliberately Turkish. CORE §6 records that a base-14 font in a
//! Latin encoding silently drops `ğ ş ı İ`, and while the clipboard is not the
//! PDF encoder, "the text came back byte-identical" is a stronger statement
//! when the text has characters that survive a naive round-trip badly. `ı`
//! (dotless i) is the one that matters: Turkish maps `i`→`İ` and `ı`→`I`, so a
//! clipboard that normalises case anywhere would corrupt it.

fn main() {
    // A serial number is the realistic payload — it is what the strip under the
    // viewer copies — and the Turkish product name is what makes the
    // round-trip prove more than ASCII would.
    const PAYLOAD: &str = "Şarj Cihazı — ABC123XYZ — ığüşöç İĞÜŞÖÇ";

    let mut clipboard = match arboard::Clipboard::new() {
        Ok(clipboard) => clipboard,
        Err(error) => {
            eprintln!("CLIPBOARD SMOKE: FAIL — could not open the clipboard: {error}");
            std::process::exit(1);
        }
    };

    if let Err(error) = clipboard.set_text(PAYLOAD) {
        eprintln!("CLIPBOARD SMOKE: FAIL — could not write: {error}");
        std::process::exit(1);
    }

    match clipboard.get_text() {
        Ok(read_back) if read_back == PAYLOAD => {
            println!(
                "CLIPBOARD SMOKE: PASS — {} bytes round-tripped byte-identical.",
                PAYLOAD.len()
            );
        }
        Ok(read_back) => {
            // Not a crash and not a pass: the clipboard worked and changed the
            // text. That is worth distinguishing, because it is the failure a
            // Turkish product name would hit and an ASCII test would miss.
            eprintln!("CLIPBOARD SMOKE: FAIL — text changed in transit.");
            eprintln!("  wrote: {PAYLOAD:?}");
            eprintln!("  read:  {read_back:?}");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("CLIPBOARD SMOKE: FAIL — could not read back: {error}");
            std::process::exit(1);
        }
    }
}
