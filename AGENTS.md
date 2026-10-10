# tally

A static Rust binary that runs a program under samply and prints the recording as text, each function's share of the samples as the leaf of the stack and anywhere on it. `README.md` says what it is for; `tally --help`, generated from `src/report/mod.rs`, is the grammar.

The engineering style guide at `~/.claude/CLAUDE.md` governs. This file states only what is specific to this repository.

## Running things

`just check` is the gate and must stay green: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` under the lint table in `Cargo.toml`, and `cargo test`. The recipes run inside the flake's shell, entering `nix develop` themselves unless `TALLY_SHELL` shows they already stand in it. `just tally <arguments>` runs the binary from the tree.

The toolchain is pinned once, in `rust-toolchain.toml`, which both the flake and CI's rustup read. The flake's shell carries samply, and its package wraps the binary so the samply its lock pins stands first on the path.

`tally record` needs perf events on Linux: `kernel.perf_event_paranoid` at 1 or lower, and on a host of many CPUs `kernel.perf_event_mlock_kb` at 1028 or more. A CI runner grants neither, so the gate covers `summarize` against fixture files and `record` is checked by hand: record a program built with symbols and a GNU build-id, and read the tables.

## Shape

`src/lib.rs` is the functional core and does no IO: the argument grammar (`cli`), the two files a recording is (`recording`), the summary of a recording's samples (`summary`), every message (`report`), and the exit codes (`exit_code`). `src/main.rs` is the shell: it runs samply, reads the recording under a size bound, prints what the library rendered, and maps outcomes to exit codes. A new decision goes in the library with a unit test; `main.rs` holds wiring, which `tests/cli.rs` covers against real files.

The summary reads the symbols sidecar `samply record --unstable-presymbolicate` writes, a format samply marks unstable. Every field it reads is checked, so a samply that moves the format exits on the format naming the field rather than printing a wrong table. Moving the flake's nixpkgs moves samply: record a program by hand after the move, before trusting the tables.

The lint table denies `unwrap`, `expect`, `panic!`, indexing, slicing, string slicing, `as` casts, and unchecked arithmetic. A programmer error crashes through `assert!` or `unreachable!` with a message; arithmetic goes through `checked_*` with a `let ... else { unreachable!(..) }`, and slicing through `.get(..)`.

## Tests

A test is a function named `when_<operation>_then_<result>` in a module named `given_<input>`, and each test file opens with its goal and method as a `//!` comment. A table of cases is written with the `rows!` macro in `src/testing.rs`, one test function per row. Each case asserts on a returned value, a refusal's reason, or what the binary printed.
