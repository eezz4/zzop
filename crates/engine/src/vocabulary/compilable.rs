//! Does the vocabulary this run was handed actually compile?
//!
//! ## The defect
//! Declare `vocabulary.authGuardPattern: "(?i)((((zorp["` and the run produces `configWarnings: []`,
//! exit 0, and output BYTE-IDENTICAL to the same run with a valid pattern. Every consumer of these keys
//! compiles the string with `Regex::new(..).ok()`, so an uncompilable value becomes `None` — which is
//! the same value an UNDECLARED key has, and undeclared means "make no judgment". The project is then
//! judged by a pattern it never wrote, and told nothing.
//!
//! What makes it worse than an ordinary typo is why these keys exist at all. Convention vocabulary was
//! moved into config precisely because zzop must not guess what a project calls its guards — so the one
//! project that took the trouble to declare, and fat-fingered a bracket, gets exactly the treatment of
//! the project that declared nothing. The key NAME is already checked carefully (an unknown key is a
//! warning listing the valid ones); only the VALUE went unexamined.
//!
//! ## Deriving the key set instead of listing it
//! The checked keys are not hand-listed here — that list would be a census with nothing keeping it
//! honest, and the next `*Pattern` key added to `VocabularyConfig` would silently fall outside it,
//! reproducing this exact defect one key over. Instead the config is serialized and every camelCase key
//! ENDING IN `Pattern` whose value is a string is compiled. That selects the regex-valued scalars by
//! construction and excludes `extraTestPathPatterns` (plural, and a list) which already validates its
//! own entries where it is applied.
//!
//! ## Why a warning and not an error
//! Same reasoning the rest of the config surface uses: a bad declaration should cost the run its
//! judgment on that ONE key and nothing else. The warning has to say that plainly, because the
//! surprising half is not that the pattern failed — it is that the run continued and read as clean.

use regex::Regex;

use super::VocabularyConfig;

/// One warning per declared pattern key that does not compile. Empty on every healthy run, including
/// one that declares no vocabulary at all.
pub fn uncompilable_vocabulary_warnings(vocab: &VocabularyConfig) -> Vec<String> {
    let Ok(serde_json::Value::Object(map)) = serde_json::to_value(vocab) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, value) in &map {
        if !key.ends_with("Pattern") {
            continue;
        }
        let Some(pattern) = value.as_str() else {
            continue;
        };
        if let Err(e) = Regex::new(pattern) {
            out.push(warning(key, pattern, &e.to_string()));
        }
    }
    out
}

/// The message. It leads with the CONSEQUENCE rather than the syntax error, because the syntax error
/// is the part the author can already see once they know to look — what they cannot see is that the
/// run went on without them.
fn warning(key: &str, pattern: &str, detail: &str) -> String {
    // `regex`'s Display is multi-line with an ASCII-art caret; flattened so the warning stays one line
    // like every other entry on this channel.
    let flat = detail.split_whitespace().collect::<Vec<_>>().join(" ");
    format!(
        "vocabulary.{key} is not a valid regular expression and was IGNORED, so this run made no \
         judgment that depends on it — exactly as if the key had never been declared. That is the part \
         worth noticing: the output looks clean rather than failing, and a project that declared this \
         key did so because the built-in answer was wrong for it. Declared value: \"{pattern}\". Why it \
         will not compile: {flat}"
    )
}

#[cfg(test)]
mod tests;
