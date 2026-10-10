//! Every message tally prints, rendered from the library's values. Pure, so each message
//! is built from values a test constructs.

use std::fmt::Display;

use crate::cli::UsageFault;
use crate::summary::FormatFault;

/// The whole grammar, which is what an agent reads before its first call.
pub const HELP: &str = "\
tally: record a program under samply and print where its samples fall, as text.

Usage:
  tally record [--limit COUNT] [--rate HERTZ] [--output PATH] [--] PROGRAM [ARGUMENT...]
      Run PROGRAM under samply, discard its stdout, and print the summary of what was
      recorded. Flags lead; `--` or the first word that is not one starts the program,
      and every word after it is the program's. --rate sets the samples per second
      (default 4000, at most 10000). --output names the recording (default
      profile.json); its symbols land beside it as <stem>.syms.json.
  tally summarize [--limit COUNT] RECORDING
      Print the summary of a recording `tally record` made, or one samply made with
      --unstable-presymbolicate and an --output ending .json.

The summary counts the samples, then lists functions heaviest first in two tables:
self, the share of samples whose innermost frame is the function, and inclusive, the
share holding the function anywhere on the stack. A share is floored to a tenth of a
percent. --limit sets the rows per table (default 25, at most 1000). samply does not
pass on the program's exit status, so tally does not print it.

Build the program with its symbols kept, such as a release profile with
`debug = \"line-tables-only\"` and `strip = \"none\"`. On Linux, samply finds a file's
symbols by its GNU build-id, which a Rust program gains linked with
`-C link-arg=-Wl,--build-id`; frames in a file without one are named for the file
alone, and stderr names it. samply needs `kernel.perf_event_paranoid` at 1 or lower.

Exit status:
  0  printed
  1  the recording does not hold what the summary reads
  2  the arguments are malformed
  3  a file of the recording could not be read or removed
  4  samply could not be started, or ended in failure
";

/// The line naming where a recording landed, printed above its summary.
#[must_use]
pub fn recorded_at(profile: &str) -> String {
    format!("recorded at {profile}\n")
}

#[must_use]
pub fn usage_fault(fault: &UsageFault) -> String {
    let reason = match fault {
        UsageFault::MissingCommand => "no command was given".to_owned(),
        UsageFault::UnknownCommand(word) => format!("{word:?} is not a command"),
        UsageFault::UnknownFlag(flag) => format!("{flag} is not a flag here"),
        UsageFault::MissingValue(flag) => format!("{flag} takes a value"),
        UsageFault::RepeatedFlag(flag) => format!("{flag} is given twice"),
        UsageFault::NotACount { flag, value } => {
            format!("{flag} takes a whole number: {value:?}")
        }
        UsageFault::ZeroCount(flag) => format!("{flag} takes a number above zero"),
        UsageFault::CountAboveMax { flag, max } => format!("{flag} takes at most {max}"),
        UsageFault::NotJson(path) => format!("{path} does not name a file ending .json"),
        UsageFault::MissingProgram => "record names no program to run".to_owned(),
        UsageFault::MissingRecording => "summarize names no recording".to_owned(),
        UsageFault::ExtraRecording(word) => {
            format!("summarize takes one recording, and {word:?} is a second")
        }
    };
    format!("tally: {reason}\nRun `tally --help` for the grammar.\n")
}

/// The note naming the libraries whose frames carry no symbols because samply found no
/// code identifier for them, which on Linux is a GNU build-id.
///
/// # Panics
///
/// Panics when `names` is empty.
#[must_use]
pub fn unidentified(names: &[String]) -> String {
    assert!(!names.is_empty(), "a note was asked for no library");
    let listed = names.join(", ");
    format!(
        "tally: samply found no build-id in {listed}, so frames there are named for the library alone; a Rust program gains one linked with `-C link-arg=-Wl,--build-id`\n"
    )
}

#[must_use]
pub fn format_fault(path: &str, fault: &FormatFault) -> String {
    format!(
        "tally: {path} does not hold what the summary reads: {}\n",
        fault.0
    )
}

#[must_use]
pub fn not_json(path: &str, error: &impl Display) -> String {
    format!("tally: {path} is not JSON: {error}\n")
}

#[must_use]
pub fn read_failure(path: &str, error: &impl Display) -> String {
    format!("tally: {path} could not be read: {error}\n")
}

#[must_use]
pub fn too_large(path: &str, bytes_max: u64) -> String {
    format!("tally: {path} holds more than the {bytes_max} bytes read\n")
}

#[must_use]
pub fn remove_failure(path: &str, error: &impl Display) -> String {
    format!("tally: the stale {path} could not be removed: {error}\n")
}

#[must_use]
pub fn recorder_not_started(error: &impl Display) -> String {
    format!("tally: samply could not be started: {error}\n")
}

#[must_use]
pub fn recorder_failed(status: &impl Display) -> String {
    format!("tally: samply ended with {status}\n")
}
