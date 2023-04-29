use crate::FORBIDDEN_USERS;

use radix_trie::{Trie, TrieCommon};
use rustyline::completion::Completer;
use rustyline::Helper;
use rustyline::highlight::Highlighter;
use rustyline::hint::{Hint, Hinter};
use rustyline::validate::Validator;

#[derive(Debug)]
pub struct Hintererer {
    commands: Trie<&'static str, Completion>,
}

#[derive(Debug)]
pub struct Completion {
    display: String,
    rem_len: usize,
}

impl Hint for Completion {
    fn completion(&self) -> Option<&str> {
        if self.rem_len > 0 {
            Some(&self.display[..self.rem_len])
        } else {
            None
        }
    }

    fn display(&self) -> &str {
        &self.display
    }
}

impl Completion {
    fn new(text: &str, complete_up_to: &str) -> Self {
        assert!(text.starts_with(complete_up_to));
        Self {
            display: text.into(),
            rem_len: complete_up_to.len(),
        }
    }

    fn suffix(&self, strip_chars: usize) -> Self {
        Self {
            display: self.display[strip_chars..].to_owned(),
            rem_len: self.rem_len.saturating_sub(strip_chars),
        }
    }
}

impl Hintererer {
    pub fn new() -> Self {
        Self {
            commands: Self::load_cmds(),
        }
    }

    pub fn load_cmds() -> Trie<&'static str, Completion> {
        let mut tr = Trie::new();

        for cmd in FORBIDDEN_USERS {
            tr.insert(cmd, Completion::new(cmd, cmd));
        }

        tr
    }
}

impl Highlighter for Hintererer {}
impl Validator for Hintererer {}
impl Helper for Hintererer {}

impl Hinter for Hintererer {
    type Hint = Completion;
    fn hint(&self, line: &str, pos: usize, _ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
        if line.is_empty() || pos < line.len() {
            None
        } else {
            self.commands.iter().find_map(|c| {
                if c.0.starts_with(line) {
                    Some(c.1.suffix(pos))
                } else {
                    None
                }
            })
        }
    }
}

impl Completer for Hintererer {
    type Candidate = String;
    fn complete(
        &self,
        line: &str,
        _pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        // let mut hints = Vec::new();
        let hints = self
            .commands
            .iter()
            .filter_map(|c| {
                if c.0.starts_with(line) {
                    Some(c.1.display().to_string())
                } else {
                    None
                }
            })
            .collect();

        Ok((0, hints))
    }
}
