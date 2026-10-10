//! The imperative shell: runs samply, reads the recording, prints what the library
//! rendered, and turns every outcome into an exit code. Every decision it acts on is made
//! by the library.

use std::fs;
use std::io::{self, Read, Write};
use std::process::{Command, ExitCode, ExitStatus, Stdio};

use serde_json::Value;
use tally::cli::{self, Record};
use tally::exit_code;
use tally::recording::Recording;
use tally::report;
use tally::summary;

/// The largest file of a recording read.
const RECORDING_BYTES_MAX: u64 = 1 << 30;
const RECORDER_PROGRAM: &str = "samply";

/// A step that ended the run: the exit code and the message naming why.
struct Failure {
    code: u8,
    message: String,
}

/// A rendered summary and the note on stderr beside it, empty where there is none.
struct Summarized {
    text: String,
    note: String,
}

/// A file's text, or the outcome that it holds more than the bound.
enum Contents {
    Text(String),
    TooLarge,
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let code = match cli::parse_arguments(&arguments) {
        Ok(cli::Command::Help) => emit(report::HELP, "", exit_code::SUCCESS),
        Ok(cli::Command::Version) => {
            let version = format!("tally {}\n", env!("CARGO_PKG_VERSION"));
            emit(&version, "", exit_code::SUCCESS)
        }
        Ok(cli::Command::Record(request)) => run_record(&request),
        Ok(cli::Command::Summarize { limit, recording }) => run_summarize(limit, &recording),
        Err(fault) => emit("", &report::usage_fault(&fault), exit_code::USAGE),
    };
    ExitCode::from(code)
}

/// Removes what an earlier run left at the recording's paths, so a run samply fails
/// to record never summarizes the one before it, then records and summarizes.
fn run_record(request: &Record) -> u8 {
    assert_ne!(
        request.recording.profile, request.recording.sidecar,
        "a recording's sidecar is named the same as its profile"
    );
    for path in [&request.recording.profile, &request.recording.sidecar] {
        if let Err(failure) = remove_stale(path) {
            return emit("", &failure.message, failure.code);
        }
    }
    if let Err(failure) = record(request) {
        return emit("", &failure.message, failure.code);
    }
    let header = report::recorded_at(&request.recording.profile);
    match summarized(request.limit, &request.recording) {
        Ok(summarized) => {
            let output = format!("{header}{}", summarized.text);
            emit(&output, &summarized.note, exit_code::SUCCESS)
        }
        Err(failure) => emit(&header, &failure.message, failure.code),
    }
}

fn run_summarize(limit: usize, recording: &Recording) -> u8 {
    match summarized(limit, recording) {
        Ok(summarized) => emit(&summarized.text, &summarized.note, exit_code::SUCCESS),
        Err(failure) => emit("", &failure.message, failure.code),
    }
}

/// Runs the program under samply. The program's stdout is discarded so the summary is
/// all that prints; its stdin and stderr are the terminal's.
fn record(request: &Record) -> Result<(), Failure> {
    let rate = request.rate_hertz.to_string();
    let started = Command::new(RECORDER_PROGRAM)
        .args([
            "record",
            "--save-only",
            "--unstable-presymbolicate",
            "--rate",
        ])
        .arg(&rate)
        .arg("--output")
        .arg(&request.recording.profile)
        .arg("--")
        .args(&request.program)
        .stdin(Stdio::inherit())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status();
    let status: ExitStatus = match started {
        Ok(status) => status,
        Err(error) => {
            return Err(Failure {
                code: exit_code::RECORDER,
                message: report::recorder_not_started(&error),
            });
        }
    };
    if status.success() {
        return Ok(());
    }
    Err(Failure {
        code: exit_code::RECORDER,
        message: report::recorder_failed(&status),
    })
}

fn summarized(limit: usize, recording: &Recording) -> Result<Summarized, Failure> {
    let profile = read_json(&recording.profile)?;
    let symbols = read_json(&recording.sidecar)?;
    let summary = match summary::summary_from(&profile, &symbols) {
        Ok(summary) => summary,
        Err(fault) => {
            return Err(Failure {
                code: exit_code::FORMAT,
                message: report::format_fault(&recording.profile, &fault),
            });
        }
    };
    let text = summary::rendered(&summary, limit);
    if summary.unidentified.is_empty() {
        return Ok(Summarized {
            text,
            note: String::new(),
        });
    }
    let note = report::unidentified(&summary.unidentified);
    Ok(Summarized { text, note })
}

fn read_json(path: &str) -> Result<Value, Failure> {
    let text = match read_bounded(path) {
        Ok(text) => text,
        Err(error) => {
            return Err(Failure {
                code: exit_code::IO,
                message: report::read_failure(path, &error),
            });
        }
    };
    let Contents::Text(text) = text else {
        return Err(Failure {
            code: exit_code::IO,
            message: report::too_large(path, RECORDING_BYTES_MAX),
        });
    };
    match serde_json::from_str(&text) {
        Ok(value) => Ok(value),
        Err(error) => Err(Failure {
            code: exit_code::FORMAT,
            message: report::not_json(path, &error),
        }),
    }
}

/// One byte past the bound is read, so a file of exactly the bound is told from a
/// longer one.
fn read_bounded(path: &str) -> io::Result<Contents> {
    let file = fs::File::open(path)?;
    let Some(limit) = RECORDING_BYTES_MAX.checked_add(1) else {
        unreachable!("the bound is far below the largest u64");
    };
    let mut text = String::new();
    let read = file.take(limit).read_to_string(&mut text)?;
    let Ok(read) = u64::try_from(read) else {
        unreachable!("a read bounded by a u64 fits a u64");
    };
    if read > RECORDING_BYTES_MAX {
        return Ok(Contents::TooLarge);
    }
    Ok(Contents::Text(text))
}

fn remove_stale(path: &str) -> Result<(), Failure> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Failure {
            code: exit_code::IO,
            message: report::remove_failure(path, &error),
        }),
    }
}

/// Writes the output and the messages and returns `code`, or the IO code when either
/// cannot be written for any reason but a reader that stopped reading.
fn emit(output: &str, messages: &str, code: u8) -> u8 {
    let mut code = code;
    if !output.is_empty() {
        let written = io::stdout().lock().write_all(output.as_bytes());
        if failed_for_a_reader(written) {
            code = exit_code::IO;
        }
    }
    if !messages.is_empty() {
        let written = io::stderr().lock().write_all(messages.as_bytes());
        if failed_for_a_reader(written) {
            code = exit_code::IO;
        }
    }
    code
}

/// Whether a write failed while its reader was still reading; a closed pipe means the
/// reader took what it wanted, as `| head` does.
fn failed_for_a_reader(written: io::Result<()>) -> bool {
    match written {
        Ok(()) => false,
        Err(error) => error.kind() != io::ErrorKind::BrokenPipe,
    }
}
