//! The text summary of a profile `samply` recorded.
//!
//! The summary is the share of samples each function holds as the leaf of the stack
//! and anywhere on it. Everything here is pure over materialized JSON values and opens
//! nothing, so the summary replays against a fixture and the shell in `main` is what
//! records and reads.
//!
//! The symbols come from the sidecar `samply record --unstable-presymbolicate` writes,
//! a format samply marks unstable; every field read here is checked, so a samply that
//! moves the format faults naming the field rather than printing a wrong table.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

const STACK_DEPTH_MAX: usize = 10_000;
const HASH_SUFFIX_MARKER: &str = "::h";
const HASH_SUFFIX_DIGITS: usize = 16;
const PERMILLE_PER_WHOLE: u64 = 1_000;
const PERMILLE_PER_PERCENT: u64 = 10;
const NO_INDEX: i64 = -1;

/// A recording that does not hold what the summary reads, naming the field it lacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatFault(pub String);

/// Which samples the summary counts: every one recorded, or those holding a function on
/// their stack, each cut at the root-most frame of that function so what lies under it
/// is all that is counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Whole,
    Under(String),
}

/// A function and the samples it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weight {
    pub name: String,
    pub samples: u64,
}

/// The weight each function holds across every recorded thread.
///
/// The samples counted in scope, every sample recorded, the samples whose leaf frame is
/// the function, and the samples holding the function anywhere on the stack, each list
/// heaviest first and alphabetical among equals, and the libraries samply found no code
/// identifier for, whose frames are named for the library alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub scope: Scope,
    pub total: u64,
    pub recorded: u64,
    pub self_weights: Vec<Weight>,
    pub inclusive_weights: Vec<Weight>,
    pub unidentified: Vec<String>,
}

#[derive(Default)]
struct Tally {
    total: u64,
    recorded: u64,
    self_weights: HashMap<String, u64>,
    inclusive_weights: HashMap<String, u64>,
    sampled_libraries: HashSet<usize>,
}

/// A frame's function name and the index of the library it lies in, where it names one.
struct Frame {
    name: String,
    library: Option<usize>,
}

struct Symbol {
    start: u64,
    end: u64,
    name: String,
}

struct Library {
    name: String,
    code_id: Option<String>,
}

struct StackTable {
    frame: Vec<usize>,
    prefix: Vec<Option<usize>>,
}

/// The weights of every function across every thread of `profile` within `scope`, each
/// frame named through the symbols in `symbols`.
///
/// # Errors
///
/// Returns a `FormatFault` naming the first field either value lacks or holds out of
/// shape.
///
/// # Panics
///
/// Panics when more samples are counted in scope than were recorded.
pub fn summary_from(
    profile: &Value,
    symbols: &Value,
    scope: &Scope,
) -> Result<Summary, FormatFault> {
    let tables = symbol_tables(symbols)?;
    let libraries = libraries_of(profile)?;
    let threads = array_at(profile, "threads", "the profile")?;
    let mut tally = Tally::default();
    for thread in threads {
        let thread_tally = thread_tally(thread, &libraries, &tables, scope)?;
        tally.total = sum(tally.total, thread_tally.total)?;
        tally.recorded = sum(tally.recorded, thread_tally.recorded)?;
        for (name, weight) in thread_tally.self_weights {
            add_weight(&mut tally.self_weights, name, weight)?;
        }
        for (name, weight) in thread_tally.inclusive_weights {
            add_weight(&mut tally.inclusive_weights, name, weight)?;
        }
        tally
            .sampled_libraries
            .extend(thread_tally.sampled_libraries);
    }
    assert!(
        tally.total <= tally.recorded,
        "more samples were counted in scope than were recorded"
    );
    Ok(Summary {
        scope: scope.clone(),
        total: tally.total,
        recorded: tally.recorded,
        self_weights: heaviest_first(tally.self_weights),
        inclusive_weights: heaviest_first(tally.inclusive_weights),
        unidentified: unidentified(&libraries, &tally.sampled_libraries),
    })
}

/// The names of the sampled libraries carrying no code identifier, each once, in order.
/// samply writes a null identifier for an ELF file without a GNU build-id note, and keys
/// the sidecar's symbols by that identifier, so such a library's frames have no symbols.
fn unidentified(libraries: &[Library], sampled: &HashSet<usize>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (index, library) in libraries.iter().enumerate() {
        if sampled.contains(&index) {
            if library.code_id.is_none() {
                names.push(library.name.clone());
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

/// The samples counted, then the two tables, each cut to `limit` rows, every row a share
/// of the samples counted floored to a tenth of a percent, and a row that floors to none
/// left out.
///
/// # Panics
///
/// Panics when `limit` is zero, a weight exceeds the samples counted, or the samples
/// counted exceed the samples recorded.
#[must_use]
pub fn rendered(summary: &Summary, limit: usize) -> String {
    assert!(limit > 0, "a table of no rows was requested");
    assert!(
        summary.total <= summary.recorded,
        "a summary counts more samples than were recorded"
    );
    let mut text = match &summary.scope {
        Scope::Whole => format!("samples: {}\n", summary.total),
        Scope::Under(name) => format!(
            "samples: {} of {} under {name}\n",
            summary.total, summary.recorded
        ),
    };
    for (title, weights) in [
        ("self", &summary.self_weights),
        ("inclusive", &summary.inclusive_weights),
    ] {
        text.push_str(title);
        text.push('\n');
        for weight in weights.iter().take(limit) {
            let permille = share_permille(weight.samples, summary.total);
            // The rows are heaviest first, so the first below a tenth of a percent ends
            // the table.
            if permille == 0 {
                break;
            }
            let share = permille_text(permille);
            text.push_str(&share);
            text.push_str("  ");
            text.push_str(&weight.name);
            text.push('\n');
        }
    }
    text
}

/// The share `weight` holds of `total`, in tenths of a percent, floored.
fn share_permille(weight: u64, total: u64) -> u64 {
    assert!(weight <= total, "a weight exceeds the samples counted");
    let Some(scaled) = weight.checked_mul(PERMILLE_PER_WHOLE) else {
        unreachable!("a sample count times a thousand fits a u64");
    };
    // Integer division floors, so a share never reads above what was sampled.
    let Some(permille) = scaled.checked_div(total) else {
        unreachable!("a weight is only shared out of a nonzero total");
    };
    assert!(permille <= PERMILLE_PER_WHOLE, "a share exceeds the whole");
    permille
}

fn permille_text(permille: u64) -> String {
    assert!(permille <= PERMILLE_PER_WHOLE, "a share exceeds the whole");
    let Some(whole) = permille.checked_div(PERMILLE_PER_PERCENT) else {
        unreachable!("the divisor is a nonzero constant");
    };
    let Some(tenth) = permille.checked_rem(PERMILLE_PER_PERCENT) else {
        unreachable!("the divisor is a nonzero constant");
    };
    format!("{whole:>3}.{tenth}%")
}

fn sum(left: u64, right: u64) -> Result<u64, FormatFault> {
    match left.checked_add(right) {
        Some(total) => Ok(total),
        None => Err(FormatFault("the sample weights overflow a u64".to_owned())),
    }
}

fn add_weight(
    weights: &mut HashMap<String, u64>,
    name: String,
    weight: u64,
) -> Result<(), FormatFault> {
    let entry = weights.entry(name).or_default();
    *entry = sum(*entry, weight)?;
    Ok(())
}

fn heaviest_first(weights: HashMap<String, u64>) -> Vec<Weight> {
    let mut ordered: Vec<Weight> = weights
        .into_iter()
        .map(|(name, samples)| Weight { name, samples })
        .collect();
    ordered.sort_by(|left, right| {
        right
            .samples
            .cmp(&left.samples)
            .then_with(|| left.name.cmp(&right.name))
    });
    ordered
}

fn thread_tally(
    thread: &Value,
    libraries: &[Library],
    tables: &HashMap<String, Vec<Symbol>>,
    scope: &Scope,
) -> Result<Tally, FormatFault> {
    let frames = frame_names(thread, libraries, tables)?;
    let stacks = stack_table(thread, frames.len())?;
    let samples = field(thread, "samples", "a thread")?;
    let sample_stacks = optional_indices_at(samples, "stack", "a thread's samples")?;
    let weights = sample_weights(samples, sample_stacks.len())?;
    let mut tally = Tally::default();
    for (stack, weight) in sample_stacks.iter().zip(weights) {
        let Some(stack) = *stack else {
            continue;
        };
        let chain = frames_of(stack, &stacks)?;
        let named = named_frames(&chain, &frames)?;
        tally.recorded = sum(tally.recorded, weight)?;
        let Some(kept) = kept_frames(&named, scope) else {
            continue;
        };
        tally.total = sum(tally.total, weight)?;
        count_sample(&mut tally, kept, weight)?;
    }
    Ok(tally)
}

/// Each frame of `chain` resolved through the frame table, leaf first.
fn named_frames<'a>(chain: &[usize], frames: &'a [Frame]) -> Result<Vec<&'a Frame>, FormatFault> {
    let mut named = Vec::with_capacity(chain.len());
    for index in chain {
        let frame = checked_get(frames, *index, "frame")?;
        named.push(frame);
    }
    Ok(named)
}

/// The frames of `chain` the scope counts: every one under the whole program, or the
/// leaf through the root-most frame named by `Under`, and none where no frame is.
fn kept_frames<'a>(chain: &'a [&'a Frame], scope: &Scope) -> Option<&'a [&'a Frame]> {
    let Scope::Under(name) = scope else {
        return Some(chain);
    };
    let anchor = chain.iter().rposition(|frame| frame.name == *name)?;
    let Some(count) = anchor.checked_add(1) else {
        unreachable!("a position within a chain is below its length");
    };
    let Some(kept) = chain.get(..count) else {
        unreachable!("a count no greater than the chain's length cuts it");
    };
    Some(kept)
}

/// Counts one sample of `weight` whose kept frames are `kept`, leaf first: once for its
/// leaf and once for each distinct function on it.
fn count_sample(tally: &mut Tally, kept: &[&Frame], weight: u64) -> Result<(), FormatFault> {
    let Some(leaf) = kept.first() else {
        unreachable!("a kept stack holds at least its anchor");
    };
    add_weight(&mut tally.self_weights, leaf.name.clone(), weight)?;
    // A recursive function appears on its stack more than once and counts once.
    let mut seen: HashSet<&str> = HashSet::new();
    for frame in kept {
        if let Some(library) = frame.library {
            tally.sampled_libraries.insert(library);
        }
        if seen.insert(frame.name.as_str()) {
            add_weight(&mut tally.inclusive_weights, frame.name.clone(), weight)?;
        }
    }
    Ok(())
}

/// The frames of a stack, leaf first. Each prefix is checked to index an earlier
/// stack, which is what ends the walk at a root.
fn frames_of(stack: usize, stacks: &StackTable) -> Result<Vec<usize>, FormatFault> {
    let mut chain = Vec::new();
    let mut current = Some(stack);
    while let Some(index) = current {
        if chain.len() >= STACK_DEPTH_MAX {
            return Err(FormatFault(format!(
                "a stack is deeper than {STACK_DEPTH_MAX} frames"
            )));
        }
        let frame = checked_get(&stacks.frame, index, "stack")?;
        let prefix = *checked_get(&stacks.prefix, index, "stack")?;
        chain.push(*frame);
        if let Some(parent) = prefix {
            if parent >= index {
                return Err(FormatFault(format!(
                    "stack {index} names a prefix that is not earlier: {parent}"
                )));
            }
        }
        current = prefix;
    }
    assert!(!chain.is_empty(), "a sampled stack holds no frame");
    Ok(chain)
}

fn stack_table(thread: &Value, frame_count: usize) -> Result<StackTable, FormatFault> {
    let table = field(thread, "stackTable", "a thread")?;
    let frame = indices_at(table, "frame", "a thread's stack table")?;
    let prefix = optional_indices_at(table, "prefix", "a thread's stack table")?;
    if frame.len() != prefix.len() {
        return Err(FormatFault(
            "a thread's stack table holds frames and prefixes of unequal length".to_owned(),
        ));
    }
    for index in &frame {
        if *index >= frame_count {
            return Err(FormatFault(format!(
                "a stack names a frame past the frame table: {index}"
            )));
        }
    }
    Ok(StackTable { frame, prefix })
}

/// One weight per sample; samples that carry no weight array weigh one each.
fn sample_weights(samples: &Value, count: usize) -> Result<Vec<u64>, FormatFault> {
    let weights = match samples.get("weight") {
        None | Some(Value::Null) => return Ok(vec![1; count]),
        Some(weights) => weights,
    };
    let Some(weights) = weights.as_array() else {
        return Err(FormatFault(
            "a thread's sample weights are not an array".to_owned(),
        ));
    };
    if weights.len() != count {
        return Err(FormatFault(
            "a thread's samples hold stacks and weights of unequal length".to_owned(),
        ));
    }
    let mut parsed = Vec::with_capacity(count);
    for weight in weights {
        let Some(weight) = weight.as_u64() else {
            return Err(FormatFault(format!(
                "a sample weight is not a count: {weight}"
            )));
        };
        parsed.push(weight);
    }
    Ok(parsed)
}

/// Each frame's function name and library: the symbol covering its address in its
/// library, or the library's name where no symbol covers it, or a marker where no
/// library is named.
fn frame_names(
    thread: &Value,
    libraries: &[Library],
    tables: &HashMap<String, Vec<Symbol>>,
) -> Result<Vec<Frame>, FormatFault> {
    let frames = field(thread, "frameTable", "a thread")?;
    let addresses = array_at(frames, "address", "a thread's frame table")?;
    let functions = indices_at(frames, "func", "a thread's frame table")?;
    let functions_table = field(thread, "funcTable", "a thread")?;
    let resources = optional_indices_at(functions_table, "resource", "a thread's function table")?;
    let resources_table = field(thread, "resourceTable", "a thread")?;
    let resource_libraries =
        optional_indices_at(resources_table, "lib", "a thread's resource table")?;
    if addresses.len() != functions.len() {
        return Err(FormatFault(
            "a thread's frame table holds addresses and functions of unequal length".to_owned(),
        ));
    }
    let mut frames = Vec::with_capacity(functions.len());
    for (address, function) in addresses.iter().zip(functions) {
        let resource = *checked_get(&resources, function, "function")?;
        let library = match resource {
            Some(resource) => *checked_get(&resource_libraries, resource, "resource")?,
            None => None,
        };
        let name = match library {
            Some(library) => {
                let library = checked_get(libraries, library, "library")?;
                symbol_name(library, address.as_u64(), tables)
            }
            None => "(no library)".to_owned(),
        };
        frames.push(Frame { name, library });
    }
    Ok(frames)
}

fn symbol_name(
    library: &Library,
    address: Option<u64>,
    tables: &HashMap<String, Vec<Symbol>>,
) -> String {
    let unresolved = format!("{} (no symbol)", library.name);
    let Some(address) = address else {
        return unresolved;
    };
    let Some(code_id) = &library.code_id else {
        return unresolved;
    };
    let Some(symbols) = tables.get(code_id) else {
        return unresolved;
    };
    let following = symbols.partition_point(|symbol| symbol.start <= address);
    let Some(covering) = following.checked_sub(1) else {
        return unresolved;
    };
    let Some(symbol) = symbols.get(covering) else {
        unreachable!("a partition point minus one indexes the table");
    };
    if address < symbol.end {
        return without_hash_suffix(&symbol.name).to_owned();
    }
    unresolved
}

/// The name without the `::h` and sixteen hex digits the legacy mangling appends.
fn without_hash_suffix(name: &str) -> &str {
    let Some((head, digits)) = name.rsplit_once(HASH_SUFFIX_MARKER) else {
        return name;
    };
    if digits.len() == HASH_SUFFIX_DIGITS {
        if digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return head;
        }
    }
    name
}

fn libraries_of(profile: &Value) -> Result<Vec<Library>, FormatFault> {
    let libraries = array_at(profile, "libs", "the profile")?;
    let mut parsed = Vec::with_capacity(libraries.len());
    for library in libraries {
        let name = string_at(library, "name", "a library")?.to_owned();
        let code_id = optional_string_at(library, "codeId", "a library")?;
        parsed.push(Library { name, code_id });
    }
    Ok(parsed)
}

/// Each library's symbols keyed by its code identifier, ordered by start address.
fn symbol_tables(symbols: &Value) -> Result<HashMap<String, Vec<Symbol>>, FormatFault> {
    let strings = array_at(symbols, "string_table", "the symbol sidecar")?;
    let libraries = array_at(symbols, "data", "the symbol sidecar")?;
    let mut tables = HashMap::with_capacity(libraries.len());
    for library in libraries {
        let code_id = string_at(library, "code_id", "a sidecar library")?.to_owned();
        let entries = array_at(library, "symbol_table", "a sidecar library")?;
        let mut table = Vec::with_capacity(entries.len());
        for entry in entries {
            let start = count_at(entry, "rva", "a sidecar symbol")?;
            let size = count_at(entry, "size", "a sidecar symbol")?;
            let string_index = count_at(entry, "symbol", "a sidecar symbol")?;
            let string_index = usize::try_from(string_index).map_err(|_| {
                FormatFault(format!(
                    "a symbol's string index is too large: {string_index}"
                ))
            })?;
            let name = checked_get(strings, string_index, "string")?;
            let Some(name) = name.as_str() else {
                return Err(FormatFault(format!(
                    "sidecar string {string_index} is not a string"
                )));
            };
            let end = start.checked_add(size).ok_or_else(|| {
                FormatFault(format!(
                    "a symbol at {start} of size {size} overflows the address space"
                ))
            })?;
            table.push(Symbol {
                start,
                end,
                name: name.to_owned(),
            });
        }
        table.sort_by_key(|symbol| symbol.start);
        tables.insert(code_id, table);
    }
    Ok(tables)
}

fn checked_get<'a, T>(items: &'a [T], index: usize, what: &str) -> Result<&'a T, FormatFault> {
    match items.get(index) {
        Some(item) => Ok(item),
        None => Err(FormatFault(format!(
            "a {what} index is past its table: {index}"
        ))),
    }
}

fn field<'a>(value: &'a Value, key: &str, owner: &str) -> Result<&'a Value, FormatFault> {
    match value.get(key) {
        Some(found) => Ok(found),
        None => Err(FormatFault(format!("{owner} carries nothing at {key:?}"))),
    }
}

fn array_at<'a>(value: &'a Value, key: &str, owner: &str) -> Result<&'a Vec<Value>, FormatFault> {
    match field(value, key, owner)?.as_array() {
        Some(items) => Ok(items),
        None => Err(FormatFault(format!("{owner} carries no array at {key:?}"))),
    }
}

fn string_at<'a>(value: &'a Value, key: &str, owner: &str) -> Result<&'a str, FormatFault> {
    match field(value, key, owner)?.as_str() {
        Some(text) => Ok(text),
        None => Err(FormatFault(format!("{owner} carries no string at {key:?}"))),
    }
}

fn count_at(value: &Value, key: &str, owner: &str) -> Result<u64, FormatFault> {
    match field(value, key, owner)?.as_u64() {
        Some(count) => Ok(count),
        None => Err(FormatFault(format!("{owner} carries no count at {key:?}"))),
    }
}

/// A string the key holds, or nothing where it holds null; a key that is absent faults.
fn optional_string_at(
    value: &Value,
    key: &str,
    owner: &str,
) -> Result<Option<String>, FormatFault> {
    match field(value, key, owner)? {
        Value::Null => Ok(None),
        Value::String(text) => Ok(Some(text.clone())),
        _ => Err(FormatFault(format!(
            "{owner} carries neither a string nor null at {key:?}"
        ))),
    }
}

fn indices_at(value: &Value, key: &str, owner: &str) -> Result<Vec<usize>, FormatFault> {
    let optional = optional_indices_at(value, key, owner)?;
    let mut indices = Vec::with_capacity(optional.len());
    for index in optional {
        let Some(index) = index else {
            return Err(FormatFault(format!(
                "{owner} holds an absent index at {key:?}"
            )));
        };
        indices.push(index);
    }
    Ok(indices)
}

/// An array of indices in which null and minus one both mark an absent index.
fn optional_indices_at(
    value: &Value,
    key: &str,
    owner: &str,
) -> Result<Vec<Option<usize>>, FormatFault> {
    let items = array_at(value, key, owner)?;
    let mut indices = Vec::with_capacity(items.len());
    for item in items {
        if item.is_null() || item.as_i64() == Some(NO_INDEX) {
            indices.push(None);
            continue;
        }
        let index = item.as_u64().and_then(|index| usize::try_from(index).ok());
        let Some(index) = index else {
            return Err(FormatFault(format!(
                "{owner} holds a value that is not an index at {key:?}: {item}"
            )));
        };
        indices.push(Some(index));
    }
    Ok(indices)
}

#[cfg(test)]
mod tests;
