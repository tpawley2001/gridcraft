//! AutoCorrect for typed text entries (Excel's File > Options > Proofing > AutoCorrect Options).

/// Which corrections run on a typed text entry, and the replacement list.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoCorrect {
    pub two_initial_caps: bool,
    pub day_names: bool,
    pub caps_lock: bool,
    pub replace_text: bool,
    pub entries: Vec<(String, String)>,
}

impl Default for AutoCorrect {
    fn default() -> Self {
        todo!("all four rules on, default_entries()")
    }
}

/// The built-in replacement list.
pub fn default_entries() -> Vec<(String, String)> {
    todo!()
}

impl AutoCorrect {
    /// The text to store for a typed entry `input`.
    pub fn correct(&self, input: &str) -> String {
        let _ = input;
        todo!()
    }
}
