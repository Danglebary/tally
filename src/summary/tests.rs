//! Goal: pin that a summary counts each sample once for its leaf and once for each
//! distinct function on its stack, resolving each frame through the sidecar's symbols,
//! and that a profile or sidecar missing what the summary reads faults rather than
//! printing a table.
//! Methodology: each case builds the JSON samply writes with only the fields this
//! module reads, over one library whose functions `main` and `parse` each cover one
//! address range, and an invalid case overrides one field of a valid thread.

use serde_json::json;

use super::*;

const MAIN_START: u64 = 0x10;
const PARSE_START: u64 = 0x20;
const FUNCTION_SIZE: u64 = 0x10;
const UNCOVERED_ADDRESS: u64 = 0x90;
const CODE_ID: &str = "A";

#[derive(Debug, Clone)]
struct ThreadFixture {
    addresses: Vec<u64>,
    stack_frames: Vec<usize>,
    stack_prefixes: Vec<Option<usize>>,
    sample_stacks: Vec<Option<usize>>,
    weights: Value,
}

/// Frame 0 is `main`, frame 1 is `parse`; stack 0 is `main` alone and stack 1 is
/// `parse` called from `main`. Two samples land in `parse` and one in `main`.
fn nested() -> ThreadFixture {
    ThreadFixture {
        addresses: vec![MAIN_START, PARSE_START],
        stack_frames: vec![0, 1],
        stack_prefixes: vec![None, Some(0)],
        sample_stacks: vec![Some(1), Some(1), Some(0)],
        weights: Value::Null,
    }
}

fn thread(fixture: &ThreadFixture) -> Value {
    let count = fixture.addresses.len();
    let functions: Vec<usize> = (0..count).collect();
    let resources = vec![0; count];
    json!({
        "frameTable": { "address": fixture.addresses, "func": functions },
        "funcTable": { "resource": resources },
        "resourceTable": { "lib": [0] },
        "stackTable": { "frame": fixture.stack_frames, "prefix": fixture.stack_prefixes },
        "samples": { "stack": fixture.sample_stacks, "weight": fixture.weights },
    })
}

fn profile_of(fixture: &ThreadFixture) -> Value {
    let thread = thread(fixture);
    json!({ "libs": [{ "name": "program", "codeId": CODE_ID }], "threads": [thread] })
}

fn symbols_named(main: &str, parse: &str) -> Value {
    json!({
        "string_table": [main, parse],
        "data": [{
            "code_id": CODE_ID,
            "symbol_table": [
                { "rva": MAIN_START, "size": FUNCTION_SIZE, "symbol": 0 },
                { "rva": PARSE_START, "size": FUNCTION_SIZE, "symbol": 1 },
            ],
        }],
    })
}

fn symbols() -> Value {
    symbols_named("main", "parse")
}

fn weights(pairs: &[(&str, u64)]) -> Vec<Weight> {
    let mut owned = Vec::with_capacity(pairs.len());
    for (name, samples) in pairs {
        owned.push(Weight {
            name: (*name).to_owned(),
            samples: *samples,
        });
    }
    owned
}

fn fault(reason: &str) -> Result<Summary, FormatFault> {
    Err(FormatFault(reason.to_owned()))
}

mod given_samples_on_a_nested_stack {
    use super::*;

    #[test]
    fn when_summarized_then_self_weights_count_each_leaf() {
        let summary = summary_from(&profile_of(&nested()), &symbols(), &Scope::Whole).ok();
        let self_weights = summary.map(|summary| summary.self_weights);
        assert_eq!(self_weights, Some(weights(&[("parse", 2), ("main", 1)])));
    }

    #[test]
    fn when_summarized_then_inclusive_weights_count_every_function_on_the_stack() {
        let summary = summary_from(&profile_of(&nested()), &symbols(), &Scope::Whole).ok();
        let inclusive = summary.map(|summary| summary.inclusive_weights);
        assert_eq!(inclusive, Some(weights(&[("main", 3), ("parse", 2)])));
    }

    #[test]
    fn when_summarized_holding_weights_then_each_sample_counts_its_weight() {
        let fixture = ThreadFixture {
            weights: json!([3, 1, 1]),
            ..nested()
        };
        let summary = summary_from(&profile_of(&fixture), &symbols(), &Scope::Whole).ok();
        let total = summary.map(|summary| summary.total);
        assert_eq!(total, Some(5));
    }

    #[test]
    fn when_summarized_holding_a_null_stack_then_that_sample_is_not_counted() {
        let fixture = ThreadFixture {
            sample_stacks: vec![Some(1), None],
            ..nested()
        };
        let summary = summary_from(&profile_of(&fixture), &symbols(), &Scope::Whole).ok();
        let total = summary.map(|summary| summary.total);
        assert_eq!(total, Some(1));
    }
}

mod given_a_recursive_stack {
    use super::*;

    #[test]
    fn when_summarized_then_the_recursing_function_counts_once_per_sample() {
        let fixture = ThreadFixture {
            addresses: vec![MAIN_START, PARSE_START, PARSE_START + 1],
            stack_frames: vec![0, 1, 2],
            stack_prefixes: vec![None, Some(0), Some(1)],
            sample_stacks: vec![Some(2)],
            ..nested()
        };
        let summary = summary_from(&profile_of(&fixture), &symbols(), &Scope::Whole).ok();
        let inclusive = summary.map(|summary| summary.inclusive_weights);
        assert_eq!(inclusive, Some(weights(&[("main", 1), ("parse", 1)])));
    }
}

mod given_a_frame_no_symbol_covers {
    use super::*;

    #[test]
    fn when_summarized_then_it_is_named_for_its_library() {
        let fixture = ThreadFixture {
            addresses: vec![MAIN_START, UNCOVERED_ADDRESS],
            ..nested()
        };
        let summary = summary_from(&profile_of(&fixture), &symbols(), &Scope::Whole).ok();
        let self_weights = summary.map(|summary| summary.self_weights);
        assert_eq!(
            self_weights,
            Some(weights(&[("program (no symbol)", 2), ("main", 1)]))
        );
    }
}

mod given_a_symbol_name_with_a_suffix {
    use super::*;

    rows! { |name: &str| summary_from(&profile_of(&nested()), &symbols_named("main", name), &Scope::Whole).ok().map(|summary| summary.self_weights[0].name.clone());
        when_summarized_holding_a_legacy_hash_then_the_hash_is_dropped: "parse::h0123456789abcdef" => Some("parse".to_owned()),
        when_summarized_holding_a_short_suffix_then_the_name_is_kept: "parse::h0123" => Some("parse::h0123".to_owned()),
        when_summarized_holding_a_non_hex_suffix_then_the_name_is_kept: "parse::hzzzzzzzzzzzzzzzz" => Some("parse::hzzzzzzzzzzzzzzzz".to_owned()),
    }
}

mod given_a_thread_missing_what_the_summary_reads {
    use super::*;

    rows! { |fixture: ThreadFixture| summary_from(&profile_of(&fixture), &symbols(), &Scope::Whole);
        when_summarized_holding_a_prefix_that_is_not_earlier_then_it_faults: ThreadFixture { stack_prefixes: vec![Some(1), Some(0)], ..nested() } => fault("stack 0 names a prefix that is not earlier: 1"),
        when_summarized_holding_a_stack_that_is_its_own_prefix_then_it_faults: ThreadFixture { stack_prefixes: vec![None, Some(1)], ..nested() } => fault("stack 1 names a prefix that is not earlier: 1"),
        when_summarized_holding_a_stack_past_the_table_then_it_faults: ThreadFixture { sample_stacks: vec![Some(2)], ..nested() } => fault("a stack index is past its table: 2"),
        when_summarized_holding_a_frame_past_the_table_then_it_faults: ThreadFixture { stack_frames: vec![0, 2], ..nested() } => fault("a stack names a frame past the frame table: 2"),
        when_summarized_holding_fewer_weights_than_samples_then_it_faults: ThreadFixture { weights: json!([1]), ..nested() } => fault("a thread's samples hold stacks and weights of unequal length"),
        when_summarized_holding_a_negative_weight_then_it_faults: ThreadFixture { weights: json!([1, -1, 1]), ..nested() } => fault("a sample weight is not a count: -1"),
    }
}

mod given_a_sidecar_missing_what_the_summary_reads {
    use super::*;

    rows! { |sidecar: Value| summary_from(&profile_of(&nested()), &sidecar, &Scope::Whole);
        when_summarized_holding_no_string_table_then_it_faults: json!({ "data": [] }) => fault("the symbol sidecar carries nothing at \"string_table\""),
        when_summarized_holding_no_data_then_it_faults: json!({ "string_table": [] }) => fault("the symbol sidecar carries nothing at \"data\""),
        when_summarized_holding_a_symbol_past_the_strings_then_it_faults: json!({ "string_table": [], "data": [{ "code_id": CODE_ID, "symbol_table": [{ "rva": MAIN_START, "size": FUNCTION_SIZE, "symbol": 0 }] }] }) => fault("a string index is past its table: 0"),
    }
}

mod given_a_profile_whose_library_carries_no_code_identifier {
    use super::*;

    #[test]
    fn when_summarized_then_it_faults() {
        let thread = thread(&nested());
        let profile = json!({ "libs": [{ "name": "program" }], "threads": [thread] });
        let observed = summary_from(&profile, &symbols(), &Scope::Whole);
        assert_eq!(observed, fault("a library carries nothing at \"codeId\""));
    }
}

mod given_a_profile_whose_library_holds_a_null_code_identifier {
    use super::*;

    fn summary() -> Option<Summary> {
        let thread = thread(&nested());
        let profile =
            json!({ "libs": [{ "name": "program", "codeId": null }], "threads": [thread] });
        summary_from(&profile, &symbols(), &Scope::Whole).ok()
    }

    #[test]
    fn when_summarized_then_its_frames_are_named_for_the_library() {
        let self_weights = summary().map(|summary| summary.self_weights);
        assert_eq!(self_weights, Some(weights(&[("program (no symbol)", 3)])));
    }

    #[test]
    fn when_summarized_then_the_library_is_named_unidentified() {
        let unidentified = summary().map(|summary| summary.unidentified);
        assert_eq!(unidentified, Some(vec!["program".to_owned()]));
    }
}

mod given_an_unsampled_library_holding_a_null_code_identifier {
    use super::*;

    #[test]
    fn when_summarized_then_no_library_is_named_unidentified() {
        let thread = thread(&nested());
        let libraries = json!([
            { "name": "program", "codeId": CODE_ID },
            { "name": "unsampled", "codeId": null },
        ]);
        let profile = json!({ "libs": libraries, "threads": [thread] });
        let summary = summary_from(&profile, &symbols(), &Scope::Whole).ok();
        let unidentified = summary.map(|summary| summary.unidentified);
        assert_eq!(unidentified, Some(Vec::new()));
    }
}

mod given_a_weight_out_of_a_total {
    use super::*;

    rows! { |(weight, total): (u64, u64)| permille_text(share_permille(weight, total));
        when_shared_holding_a_third_then_it_floors_to_the_tenth: (1, 3) => " 33.3%".to_owned(),
        when_shared_holding_two_thirds_then_it_floors_rather_than_rounds: (2, 3) => " 66.6%".to_owned(),
        when_shared_holding_the_whole_then_it_reads_one_hundred: (3, 3) => "100.0%".to_owned(),
    }
}

mod given_a_scope_under_a_function {
    use super::*;

    fn summary_under(name: &str, fixture: &ThreadFixture) -> Option<Summary> {
        let scope = Scope::Under(name.to_owned());
        summary_from(&profile_of(fixture), &symbols(), &scope).ok()
    }

    #[test]
    fn when_summarized_then_only_the_samples_holding_it_are_counted() {
        let summary = summary_under("parse", &nested());
        let counts = summary.map(|summary| (summary.total, summary.recorded));
        assert_eq!(counts, Some((2, 3)));
    }

    #[test]
    fn when_summarized_then_the_frames_above_it_are_not_counted() {
        let summary = summary_under("parse", &nested());
        let inclusive = summary.map(|summary| summary.inclusive_weights);
        assert_eq!(inclusive, Some(weights(&[("parse", 2)])));
    }

    #[test]
    fn when_summarized_holding_the_root_then_the_whole_is_counted() {
        let summary = summary_under("main", &nested());
        let inclusive = summary.map(|summary| summary.inclusive_weights);
        assert_eq!(inclusive, Some(weights(&[("main", 3), ("parse", 2)])));
    }

    /// Frame 2 lies at no symbol and frame 3 is `parse` again: the one stack is
    /// `parse` called through the unsymbolized frame from `parse` from `main`.
    #[test]
    fn when_summarized_and_it_recurses_then_the_cut_is_at_its_root_most_frame() {
        let fixture = ThreadFixture {
            addresses: vec![MAIN_START, PARSE_START, UNCOVERED_ADDRESS, PARSE_START + 1],
            stack_frames: vec![0, 1, 2, 3],
            stack_prefixes: vec![None, Some(0), Some(1), Some(2)],
            sample_stacks: vec![Some(3)],
            ..nested()
        };
        let summary = summary_under("parse", &fixture);
        let inclusive = summary.map(|summary| summary.inclusive_weights);
        assert_eq!(
            inclusive,
            Some(weights(&[("parse", 1), ("program (no symbol)", 1)]))
        );
    }

    #[test]
    fn when_summarized_and_no_sample_holds_it_then_nothing_is_counted() {
        let summary = summary_under("absent", &nested());
        let observed =
            summary.map(|summary| (summary.total, summary.recorded, summary.self_weights));
        assert_eq!(observed, Some((0, 3, Vec::new())));
    }
}

mod given_a_summary_under_a_function {
    use super::*;

    #[test]
    fn when_rendered_then_the_header_counts_the_whole_beside_the_scope() {
        let summary = Summary {
            scope: Scope::Under("a".to_owned()),
            total: 2,
            recorded: 5,
            self_weights: weights(&[("a", 2)]),
            inclusive_weights: weights(&[("a", 2)]),
            unidentified: Vec::new(),
        };
        let text = rendered(&summary, 1);
        assert_eq!(text.lines().next(), Some("samples: 2 of 5 under a"));
    }
}

mod given_a_summary_longer_than_the_limit {
    use super::*;

    #[test]
    fn when_rendered_then_each_table_holds_the_limit_in_rows() {
        let summary = Summary {
            scope: Scope::Whole,
            total: 3,
            recorded: 3,
            self_weights: weights(&[("a", 1), ("b", 1), ("c", 1)]),
            inclusive_weights: weights(&[("a", 3), ("b", 2), ("c", 1)]),
            unidentified: Vec::new(),
        };
        let text = rendered(&summary, 1);
        let row_count = text.lines().filter(|line| line.ends_with("  a")).count();
        let line_count = text.lines().count();
        assert_eq!((row_count, line_count), (2, 5));
    }
}

mod given_a_row_below_a_tenth_of_a_percent {
    use super::*;

    #[test]
    fn when_rendered_then_the_row_is_not_printed() {
        let summary = Summary {
            scope: Scope::Whole,
            total: 2_000,
            recorded: 2_000,
            self_weights: weights(&[("a", 1_999), ("b", 1)]),
            inclusive_weights: weights(&[("a", 2_000), ("b", 1)]),
            unidentified: Vec::new(),
        };
        let text = rendered(&summary, 1_000);
        assert_eq!(
            text,
            "samples: 2000\nself\n 99.9%  a\ninclusive\n100.0%  a\n"
        );
    }
}
