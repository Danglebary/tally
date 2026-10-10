//! The command line's grammar: the arguments in, the command to run or a usage fault out.

use crate::recording::{self, Recording};

/// The rows each table prints where `--limit` is absent.
pub const LIMIT_DEFAULT: u64 = 25;
/// The most rows a table prints.
pub const LIMIT_MAX: u64 = 1_000;
/// The samples per second where `--rate` is absent: at 4 kHz a half-second command
/// yields about two thousand samples.
pub const RATE_HERTZ_DEFAULT: u64 = 4_000;
/// The most samples per second a recording takes.
pub const RATE_HERTZ_MAX: u64 = 10_000;
/// The recording `record` writes where `--output` is absent: samply's own default name,
/// uncompressed, in the working directory.
pub const OUTPUT_DEFAULT: &str = "profile.json";
const LIMIT_FLAG: &str = "--limit";
const RATE_FLAG: &str = "--rate";
const OUTPUT_FLAG: &str = "--output";
const SEPARATOR: &str = "--";

/// What one invocation does.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Record(Record),
    Summarize { limit: usize, recording: Recording },
    Help,
    Version,
}

/// A program to run under samply, where its recording lands, and how much of the
/// summary to print.
#[derive(Debug, PartialEq, Eq)]
pub struct Record {
    pub limit: usize,
    pub rate_hertz: u64,
    pub recording: Recording,
    pub program: Vec<String>,
}

/// Every way the arguments break the grammar.
#[derive(Debug, PartialEq, Eq)]
pub enum UsageFault {
    MissingCommand,
    UnknownCommand(String),
    UnknownFlag(String),
    MissingValue(String),
    RepeatedFlag(String),
    NotACount { flag: String, value: String },
    ZeroCount(String),
    CountAboveMax { flag: String, max: u64 },
    NotJson(String),
    MissingProgram,
    MissingRecording,
    ExtraRecording(String),
}

/// Parses the arguments after the program name.
///
/// # Errors
///
/// Returns the first argument the grammar refuses.
pub fn parse_arguments(arguments: &[String]) -> Result<Command, UsageFault> {
    let words: Vec<&str> = arguments.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] => Err(UsageFault::MissingCommand),
        ["--help" | "-h"] => Ok(Command::Help),
        ["--version" | "-V"] => Ok(Command::Version),
        ["record", rest @ ..] => parse_record(rest),
        ["summarize", rest @ ..] => parse_summarize(rest),
        [first, ..] => Err(UsageFault::UnknownCommand((*first).to_owned())),
    }
}

/// Flags lead, each at most once; `--` or the first word that is not a flag starts the
/// program, and every word from there on is the program's, flags included.
fn parse_record(words: &[&str]) -> Result<Command, UsageFault> {
    let mut limit = None;
    let mut rate_hertz = None;
    let mut output = None;
    let mut rest = words;
    // Each arm that continues drops two words, so the loop ends within the list.
    let program = loop {
        match rest {
            [SEPARATOR, program @ ..] => break program,
            [LIMIT_FLAG, value, tail @ ..] => {
                refuse_repeat(limit.is_some(), LIMIT_FLAG)?;
                limit = Some(bounded_count(LIMIT_FLAG, value, LIMIT_MAX)?);
                rest = tail;
            }
            [RATE_FLAG, value, tail @ ..] => {
                refuse_repeat(rate_hertz.is_some(), RATE_FLAG)?;
                rate_hertz = Some(bounded_count(RATE_FLAG, value, RATE_HERTZ_MAX)?);
                rest = tail;
            }
            [OUTPUT_FLAG, value, tail @ ..] => {
                refuse_repeat(output.is_some(), OUTPUT_FLAG)?;
                output = Some(*value);
                rest = tail;
            }
            [flag @ (LIMIT_FLAG | RATE_FLAG | OUTPUT_FLAG)] => {
                return Err(UsageFault::MissingValue((*flag).to_owned()));
            }
            [word, ..] if is_flag(word) => {
                return Err(UsageFault::UnknownFlag((*word).to_owned()));
            }
            _ => break rest,
        }
    };
    if program.is_empty() {
        return Err(UsageFault::MissingProgram);
    }
    let recording = recording_named(output.unwrap_or(OUTPUT_DEFAULT))?;
    let program: Vec<String> = program.iter().map(|word| (*word).to_owned()).collect();
    Ok(Command::Record(Record {
        limit: limit_or_default(limit),
        rate_hertz: rate_hertz.unwrap_or(RATE_HERTZ_DEFAULT),
        recording,
        program,
    }))
}

/// `--limit` may stand anywhere, since agents append a flag as often as they lead with
/// it; the one other word is the recording.
fn parse_summarize(words: &[&str]) -> Result<Command, UsageFault> {
    let mut limit = None;
    let mut profile = None;
    let mut rest = words;
    // Each arm that continues drops at least one word, so the loop ends within the list.
    loop {
        match rest {
            [] => break,
            [LIMIT_FLAG, value, tail @ ..] => {
                refuse_repeat(limit.is_some(), LIMIT_FLAG)?;
                limit = Some(bounded_count(LIMIT_FLAG, value, LIMIT_MAX)?);
                rest = tail;
            }
            [LIMIT_FLAG] => return Err(UsageFault::MissingValue(LIMIT_FLAG.to_owned())),
            [word, tail @ ..] => {
                if is_flag(word) {
                    return Err(UsageFault::UnknownFlag((*word).to_owned()));
                }
                if profile.is_some() {
                    return Err(UsageFault::ExtraRecording((*word).to_owned()));
                }
                profile = Some(*word);
                rest = tail;
            }
        }
    }
    let Some(profile) = profile else {
        return Err(UsageFault::MissingRecording);
    };
    let recording = recording_named(profile)?;
    Ok(Command::Summarize {
        limit: limit_or_default(limit),
        recording,
    })
}

fn refuse_repeat(taken: bool, flag: &str) -> Result<(), UsageFault> {
    if taken {
        return Err(UsageFault::RepeatedFlag(flag.to_owned()));
    }
    Ok(())
}

fn bounded_count(flag: &str, value: &str, max: u64) -> Result<u64, UsageFault> {
    let Ok(parsed) = value.parse::<u64>() else {
        return Err(UsageFault::NotACount {
            flag: flag.to_owned(),
            value: value.to_owned(),
        });
    };
    if parsed == 0 {
        return Err(UsageFault::ZeroCount(flag.to_owned()));
    }
    if parsed > max {
        return Err(UsageFault::CountAboveMax {
            flag: flag.to_owned(),
            max,
        });
    }
    Ok(parsed)
}

fn limit_or_default(limit: Option<u64>) -> usize {
    let limit = limit.unwrap_or(LIMIT_DEFAULT);
    assert!(limit > 0, "a limit of no rows passed the grammar");
    assert!(
        limit <= LIMIT_MAX,
        "a limit past its ceiling passed the grammar"
    );
    let Ok(limit) = usize::try_from(limit) else {
        unreachable!("a limit under its ceiling fits a usize");
    };
    limit
}

fn recording_named(profile: &str) -> Result<Recording, UsageFault> {
    match recording::named(profile) {
        Ok(recording) => Ok(recording),
        Err(recording::NotJson) => Err(UsageFault::NotJson(profile.to_owned())),
    }
}

fn is_flag(word: &str) -> bool {
    word.starts_with('-')
}

#[cfg(test)]
mod tests;
