//! The binary end to end, against real files in a scratch directory per case: what it
//! prints to stdout and to stderr, and the exit code a caller turns on. The pure rules are
//! tested in the library; these cases cover the wiring of `summarize`: the reads, the
//! sidecar found beside the profile, and which stream each message lands on. `record`
//! runs samply, which needs perf events a CI runner does not grant, so its wiring is
//! left to a run by hand.

use support::{Scratch, run};

#[cfg(test)]
mod support {
    use std::fs;
    use std::path::PathBuf;
    use std::process::{Command, Output};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

    /// A directory of its own for one case, removed when the case ends.
    pub struct Scratch {
        pub root: PathBuf,
    }

    impl Scratch {
        pub fn new() -> Self {
            let index = NEXT_DIRECTORY.fetch_add(1, Ordering::SeqCst);
            let name = format!("tally-cli-{}-{index}", std::process::id());
            let root = std::env::temp_dir().join(name);
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        pub fn write(&self, name: &str, text: &str) {
            fs::write(self.root.join(name), text).unwrap();
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }

    /// Runs the binary from the scratch directory: its exit code, stdout, and stderr.
    pub fn run(scratch: &Scratch, arguments: &[&str]) -> (i32, String, String) {
        let output: Output = Command::new(env!("CARGO_BIN_EXE_tally"))
            .args(arguments)
            .current_dir(&scratch.root)
            .output()
            .unwrap();
        (
            output.status.code().unwrap(),
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap(),
        )
    }
}

/// One thread whose frame 0 is `main` and frame 1 is `parse` called from it; two
/// samples land in `parse` and one in `main`.
const PROFILE: &str = r#"{
  "libs": [{ "name": "app", "codeId": "A" }],
  "threads": [{
    "frameTable": { "address": [16, 32], "func": [0, 1] },
    "funcTable": { "resource": [0, 0] },
    "resourceTable": { "lib": [0] },
    "stackTable": { "frame": [0, 1], "prefix": [null, 0] },
    "samples": { "stack": [1, 1, 0], "weight": null }
  }]
}"#;

const SYMBOLS: &str = r#"{
  "string_table": ["main", "parse::h0123456789abcdef"],
  "data": [{
    "code_id": "A",
    "symbol_table": [
      { "rva": 16, "size": 16, "symbol": 0 },
      { "rva": 32, "size": 16, "symbol": 1 }
    ]
  }]
}"#;

mod given_a_recording_and_its_sidecar {
    use super::*;

    #[test]
    fn when_summarized_then_both_tables_print_to_stdout() {
        let scratch = Scratch::new();
        scratch.write("run.json", PROFILE);
        scratch.write("run.syms.json", SYMBOLS);
        let expected = "\
samples: 3
self
 66.6%  parse
 33.3%  main
inclusive
100.0%  main
 66.6%  parse
";
        let observed = run(&scratch, &["summarize", "run.json"]);
        assert_eq!(observed, (0, expected.to_owned(), String::new()));
    }

    #[test]
    fn when_summarized_holding_a_limit_then_each_table_holds_that_many_rows() {
        let scratch = Scratch::new();
        scratch.write("run.json", PROFILE);
        scratch.write("run.syms.json", SYMBOLS);
        let expected = "samples: 3\nself\n 66.6%  parse\ninclusive\n100.0%  main\n";
        let observed = run(&scratch, &["summarize", "run.json", "--limit", "1"]);
        assert_eq!(observed, (0, expected.to_owned(), String::new()));
    }

    #[test]
    fn when_summarized_under_a_function_then_the_header_counts_the_whole_beside_it() {
        let scratch = Scratch::new();
        scratch.write("run.json", PROFILE);
        scratch.write("run.syms.json", SYMBOLS);
        let expected = "\
samples: 2 of 3 under parse
self
100.0%  parse
inclusive
100.0%  parse
";
        let observed = run(&scratch, &["summarize", "run.json", "--under", "parse"]);
        assert_eq!(observed, (0, expected.to_owned(), String::new()));
    }
}

mod given_a_recording_without_its_sidecar {
    use super::*;

    #[test]
    fn when_summarized_then_it_exits_on_the_read_failure() {
        let scratch = Scratch::new();
        scratch.write("run.json", PROFILE);
        let (code, stdout, stderr) = run(&scratch, &["summarize", "run.json"]);
        assert_eq!((code, stdout.as_str()), (3, ""));
        assert!(stderr.starts_with("tally: run.syms.json could not be read"));
    }
}

mod given_a_recording_that_is_not_json {
    use super::*;

    #[test]
    fn when_summarized_then_it_exits_on_the_format() {
        let scratch = Scratch::new();
        scratch.write("run.json", "not json");
        scratch.write("run.syms.json", SYMBOLS);
        let (code, stdout, stderr) = run(&scratch, &["summarize", "run.json"]);
        assert_eq!((code, stdout.as_str()), (1, ""));
        assert!(stderr.starts_with("tally: run.json is not JSON"));
    }
}

mod given_a_recording_missing_what_the_summary_reads {
    use super::*;

    #[test]
    fn when_summarized_then_it_exits_on_the_format() {
        let scratch = Scratch::new();
        scratch.write("run.json", r#"{ "libs": [] }"#);
        scratch.write("run.syms.json", SYMBOLS);
        let expected = "tally: run.json does not hold what the summary reads: the profile carries nothing at \"threads\"\n";
        let observed = run(&scratch, &["summarize", "run.json"]);
        assert_eq!(observed, (1, String::new(), expected.to_owned()));
    }
}

mod given_arguments_the_grammar_refuses {
    use super::*;

    #[test]
    fn when_run_then_it_exits_on_usage_and_names_the_help() {
        let scratch = Scratch::new();
        let expected =
            "tally: record names no program to run\nRun `tally --help` for the grammar.\n";
        let observed = run(&scratch, &["record"]);
        assert_eq!(observed, (2, String::new(), expected.to_owned()));
    }
}
