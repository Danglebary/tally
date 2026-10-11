//! Goal: pin the command each argument list parses to, and the fault each refused list
//! names. Methodology: a parsed command is compared whole against one built by hand, and
//! each refused list is a row naming the fault it must produce.

use super::*;

fn whole(limit: usize) -> View {
    View {
        limit,
        scope: Scope::Whole,
    }
}

fn under(limit: usize, name: &str) -> View {
    View {
        limit,
        scope: Scope::Under(name.to_owned()),
    }
}

fn parsed(words: &[&str]) -> Result<Command, UsageFault> {
    let arguments: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
    parse_arguments(&arguments)
}

fn owned(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

fn recording(profile: &str, sidecar: &str) -> Recording {
    Recording {
        profile: profile.to_owned(),
        sidecar: sidecar.to_owned(),
    }
}

mod given_record_naming_only_a_program {
    use super::*;

    #[test]
    fn when_parsed_then_the_defaults_stand() {
        let expected = Command::Record(Record {
            view: whole(100),
            rate_hertz: 4_000,
            recording: recording("profile.json", "profile.syms.json"),
            program: owned(&["./target/release/app", "check"]),
        });
        assert_eq!(
            parsed(&["record", "./target/release/app", "check"]),
            Ok(expected)
        );
    }
}

mod given_record_leading_with_flags {
    use super::*;

    #[test]
    fn when_parsed_then_each_flag_is_taken_and_the_rest_is_the_program() {
        let words = [
            "record",
            "--rate",
            "100",
            "--limit",
            "5",
            "--under",
            "app::walk",
            "--output",
            "out/run.json",
            "app",
            "--limit",
            "9",
        ];
        let expected = Command::Record(Record {
            view: under(5, "app::walk"),
            rate_hertz: 100,
            recording: recording("out/run.json", "out/run.syms.json"),
            program: owned(&["app", "--limit", "9"]),
        });
        assert_eq!(parsed(&words), Ok(expected));
    }

    #[test]
    fn when_parsed_holding_a_separator_then_a_program_led_by_a_dash_is_the_program() {
        let expected = Command::Record(Record {
            view: whole(100),
            rate_hertz: 4_000,
            recording: recording("profile.json", "profile.syms.json"),
            program: owned(&["-app", "--rate", "1"]),
        });
        assert_eq!(
            parsed(&["record", "--", "-app", "--rate", "1"]),
            Ok(expected)
        );
    }
}

mod given_record_the_grammar_refuses {
    use super::*;

    rows! { |words: &[&str]| parsed(words);
        when_parsed_holding_no_program_then_it_is_refused: &["record", "--limit", "5"] => Err(UsageFault::MissingProgram),
        when_parsed_holding_nothing_after_the_separator_then_it_is_refused: &["record", "--"] => Err(UsageFault::MissingProgram),
        when_parsed_holding_a_flag_without_a_value_then_it_is_refused: &["record", "--rate"] => Err(UsageFault::MissingValue("--rate".to_owned())),
        when_parsed_holding_a_zero_limit_then_it_is_refused: &["record", "--limit", "0", "app"] => Err(UsageFault::ZeroCount("--limit".to_owned())),
        when_parsed_holding_a_limit_past_the_ceiling_then_it_is_refused: &["record", "--limit", "1001", "app"] => Err(UsageFault::CountAboveMax { flag: "--limit".to_owned(), max: 1_000 }),
        when_parsed_holding_a_rate_past_the_ceiling_then_it_is_refused: &["record", "--rate", "10001", "app"] => Err(UsageFault::CountAboveMax { flag: "--rate".to_owned(), max: 10_000 }),
        when_parsed_holding_a_flag_twice_then_it_is_refused: &["record", "--rate", "1", "--rate", "2", "app"] => Err(UsageFault::RepeatedFlag("--rate".to_owned())),
        when_parsed_holding_an_empty_function_name_then_it_is_refused: &["record", "--under", "", "app"] => Err(UsageFault::EmptyName("--under".to_owned())),
        when_parsed_holding_a_function_name_without_a_value_then_it_is_refused: &["record", "--under"] => Err(UsageFault::MissingValue("--under".to_owned())),
        when_parsed_holding_a_value_that_is_not_a_number_then_it_is_refused: &["record", "--limit", "many", "app"] => Err(UsageFault::NotACount { flag: "--limit".to_owned(), value: "many".to_owned() }),
        when_parsed_holding_an_output_naming_no_json_file_then_it_is_refused: &["record", "--output", "run.json.gz", "app"] => Err(UsageFault::NotJson("run.json.gz".to_owned())),
        when_parsed_holding_an_unknown_flag_then_it_is_refused: &["record", "--frequency", "1", "app"] => Err(UsageFault::UnknownFlag("--frequency".to_owned())),
    }
}

mod given_summarize {
    use super::*;

    rows! { |words: &[&str]| parsed(words);
        when_parsed_holding_a_recording_then_it_is_summarized_at_the_default_limit: &["summarize", "run.json"] => Ok(Command::Summarize { view: whole(100), recording: recording("run.json", "run.syms.json") }),
        when_parsed_holding_the_limit_after_the_recording_then_the_limit_is_taken: &["summarize", "run.json", "--limit", "3"] => Ok(Command::Summarize { view: whole(3), recording: recording("run.json", "run.syms.json") }),
        when_parsed_holding_a_function_name_then_the_scope_is_under_it: &["summarize", "--under", "app::walk", "run.json"] => Ok(Command::Summarize { view: under(100, "app::walk"), recording: recording("run.json", "run.syms.json") }),
    }
}

mod given_summarize_the_grammar_refuses {
    use super::*;

    rows! { |words: &[&str]| parsed(words);
        when_parsed_holding_no_recording_then_it_is_refused: &["summarize", "--limit", "3"] => Err(UsageFault::MissingRecording),
        when_parsed_holding_two_recordings_then_it_is_refused: &["summarize", "a.json", "b.json"] => Err(UsageFault::ExtraRecording("b.json".to_owned())),
        when_parsed_holding_a_record_flag_then_it_is_refused: &["summarize", "--rate", "1", "a.json"] => Err(UsageFault::UnknownFlag("--rate".to_owned())),
        when_parsed_holding_a_recording_naming_no_json_file_then_it_is_refused: &["summarize", "profile.json.gz"] => Err(UsageFault::NotJson("profile.json.gz".to_owned())),
        when_parsed_holding_a_limit_without_a_value_then_it_is_refused: &["summarize", "a.json", "--limit"] => Err(UsageFault::MissingValue("--limit".to_owned())),
        when_parsed_holding_a_function_name_twice_then_it_is_refused: &["summarize", "--under", "a", "--under", "b", "a.json"] => Err(UsageFault::RepeatedFlag("--under".to_owned())),
        when_parsed_holding_an_empty_function_name_then_it_is_refused: &["summarize", "--under", "", "a.json"] => Err(UsageFault::EmptyName("--under".to_owned())),
    }
}

mod given_no_subcommand {
    use super::*;

    rows! { |words: &[&str]| parsed(words);
        when_parsed_holding_nothing_then_it_is_refused: &[] => Err(UsageFault::MissingCommand),
        when_parsed_holding_an_unknown_word_then_it_is_refused: &["profile"] => Err(UsageFault::UnknownCommand("profile".to_owned())),
        when_parsed_holding_help_then_help_is_the_command: &["--help"] => Ok(Command::Help),
        when_parsed_holding_version_then_version_is_the_command: &["-V"] => Ok(Command::Version),
    }
}
