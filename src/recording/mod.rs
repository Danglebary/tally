//! The two files a recording is.
//!
//! The profile stands at the path asked for, which samply writes as plain JSON under a
//! name ending `.json`; the symbols sidecar `samply record --unstable-presymbolicate`
//! writes beside it is named for the profile with its `.json` replaced by `.syms.json`.

const PROFILE_EXTENSION: &str = ".json";
const SIDECAR_EXTENSION: &str = ".syms.json";
const PATH_SEPARATOR: char = '/';

/// A recording's profile and the sidecar samply names after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recording {
    pub profile: String,
    pub sidecar: String,
}

/// A profile path that does not name a file ending `.json`.
#[derive(Debug, PartialEq, Eq)]
pub struct NotJson;

/// The recording whose profile is at `profile`.
///
/// # Errors
///
/// Returns `NotJson` when the path does not end `.json`, or its file name is that
/// extension alone.
pub fn named(profile: &str) -> Result<Recording, NotJson> {
    let Some(stem) = profile.strip_suffix(PROFILE_EXTENSION) else {
        return Err(NotJson);
    };
    if stem.is_empty() {
        return Err(NotJson);
    }
    if stem.ends_with(PATH_SEPARATOR) {
        return Err(NotJson);
    }
    let sidecar = format!("{stem}{SIDECAR_EXTENSION}");
    Ok(Recording {
        profile: profile.to_owned(),
        sidecar,
    })
}

#[cfg(test)]
mod tests;
