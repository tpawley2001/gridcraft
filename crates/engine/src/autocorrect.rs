//! AutoCorrect for typed text entries (File > Options > Proofing).

use gridcraft_core::{Value, date::DateSystem, parse::parse_input};

/// Which corrections run on a typed text entry, and the replacement list.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoCorrect {
    pub two_initial_caps: bool,
    pub day_names: bool,
    pub caps_lock: bool,
    pub replace_text: bool,
    pub entries: Vec<(String, String)>,
    /// Words the TWo INitial CApitals rule leaves as typed (Exceptions > INitial CAps).
    pub two_caps_exceptions: Vec<String>,
}

impl Default for AutoCorrect {
    fn default() -> Self {
        Self {
            two_initial_caps: true,
            day_names: true,
            caps_lock: true,
            replace_text: true,
            entries: default_entries(),
            two_caps_exceptions: Vec::new(),
        }
    }
}

/// The built-in replacement list.
pub fn default_entries() -> Vec<(String, String)> {
    [("(c)", "©"), ("(r)", "®"), ("(tm)", "™"), ("(e)", "€"), ("...", "…"), ("teh", "the"), ("adn", "and"), ("recieve", "receive")]
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect()
}

impl AutoCorrect {
    /// The text to store for a typed entry. Parsed values and explicit text stay unchanged.
    pub fn correct(&self, input: &str) -> String {
        if input.starts_with(['=', '+', '-', '\'']) || !matches!(parse_input(input, DateSystem::D1900).value, Value::Text(_)) {
            return input.into();
        }
        let mut output = String::new();
        for token in input.split_inclusive(char::is_whitespace) {
            let body = token.trim_end_matches(char::is_whitespace);
            if let Some((_, replacement)) = self.entries.iter().find(|(key, _)| self.replace_text && key == body) {
                // A replacement is final: do not apply the word rules to its output.
                output.push_str(replacement);
            } else {
                for part in body.split_inclusive(|c: char| !c.is_alphabetic()) {
                    let word = part.trim_end_matches(|c: char| !c.is_alphabetic());
                    output.push_str(&self.correct_word(word));
                    output.push_str(part.get(word.len()..).unwrap_or_default());
                }
            }
            output.push_str(token.get(body.len()..).unwrap_or_default());
        }
        output
    }

    fn correct_word(&self, word: &str) -> String {
        let mut letters = word.chars();
        let Some(first) = letters.next() else { return String::new() };
        let Some(second) = letters.next() else { return word.into() };
        if self.caps_lock && first.is_lowercase() && second.is_uppercase() && letters.clone().all(char::is_uppercase) {
            return first.to_uppercase().chain(word.chars().skip(1).flat_map(char::to_lowercase)).collect();
        }
        if self.two_initial_caps
            && first.is_uppercase()
            && second.is_uppercase()
            && letters.clone().next().is_some()
            && letters.clone().all(char::is_lowercase)
        {
            return std::iter::once(first).chain(second.to_lowercase()).chain(letters).collect();
        }
        if self.day_names
            && word.chars().all(char::is_lowercase)
            && ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"].iter().any(|day| word.eq_ignore_ascii_case(day))
        {
            return first.to_uppercase().chain(word.chars().skip(1)).collect();
        }
        word.into()
    }
}
