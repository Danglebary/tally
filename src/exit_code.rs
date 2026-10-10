//! Every exit code tally ends with, each with one owner.

pub const SUCCESS: u8 = 0;
/// The recording does not hold what the summary reads, or a file of it is not JSON.
pub const FORMAT: u8 = 1;
/// The arguments break the grammar.
pub const USAGE: u8 = 2;
/// A file of the recording could not be read or removed.
pub const IO: u8 = 3;
/// samply could not be started, or ended in failure.
pub const RECORDER: u8 = 4;
