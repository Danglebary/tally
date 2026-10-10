//! The functional core of tally: the argument grammar, the two files a recording is,
//! the summary of a recording's samples, and every message. Nothing here runs a process,
//! reads a file, or reads the environment, so each rule is tested against values;
//! `main.rs` is the shell that does all of that.

#[cfg(test)]
#[macro_use]
mod testing;

pub mod cli;
pub mod exit_code;
pub mod recording;
pub mod report;
pub mod summary;
