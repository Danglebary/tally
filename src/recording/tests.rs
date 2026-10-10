//! Goal: pin the sidecar each profile path names and the paths refused as naming no
//! JSON file. Methodology: one row per path, the sidecar compared whole.

use super::*;

mod given_a_profile_path {
    use super::*;

    rows! { |path: &str| named(path).map(|recording| recording.sidecar);
        when_named_holding_a_bare_file_then_the_sidecar_stands_beside_it: "profile.json" => Ok("profile.syms.json".to_owned()),
        when_named_holding_a_directory_then_the_sidecar_stands_in_it: "target/profiling/run.json" => Ok("target/profiling/run.syms.json".to_owned()),
        when_named_holding_dots_in_the_stem_then_only_the_extension_is_replaced: "outline.rust.json" => Ok("outline.rust.syms.json".to_owned()),
    }
}

mod given_a_path_naming_no_json_file {
    use super::*;

    rows! { |path: &str| named(path);
        when_named_holding_a_gzipped_profile_then_it_is_refused: "profile.json.gz" => Err(NotJson),
        when_named_holding_no_extension_then_it_is_refused: "profile" => Err(NotJson),
        when_named_holding_the_extension_alone_then_it_is_refused: ".json" => Err(NotJson),
        when_named_holding_a_directory_and_the_extension_alone_then_it_is_refused: "target/.json" => Err(NotJson),
    }
}
