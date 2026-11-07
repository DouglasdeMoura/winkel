// The hero numeral's editor, the TextInput under the steppers. A digit-only
// line: typing edits, the arrows step the tempo, return commits the focus
// back to the view, escape reverts. Editing finished — on return or when
// the focus goes — is what moves the metronome, exactly as the QML's
// onEditingFinished does.

#[derive(Default)]
pub struct Hero {
    // The text as typed. The bpm property only moves on editing finished.
    pub text: String,
    // The caret, as a byte offset into text.
    pub cursor: usize,
    // Selection anchors: byte offsets, empty when none.
    pub selection: Option<(usize, usize)>,
}

impl Hero {
    pub fn set(&mut self, value: u32) {
        self.text = value.to_string();
        self.cursor = self.text.len();
        self.selection = None;
    }

    pub fn parse(&self) -> Option<u32> {
        self.text.trim().parse::<u32>().ok().filter(|v| (1..=4000).contains(v))
    }

    fn select_range(&mut self) -> (usize, usize) {
        self.selection.take().map(|(a, b)| (a.min(b), a.max(b))).unwrap_or((self.cursor, self.cursor))
    }

    // Insert one or more characters at the caret, honouring the selection.
    // The validator is IntValidator{1..4000}: digits only, and a digit
    // that would push the number past the ceiling is refused.
    pub fn insert(&mut self, s: &str) {
        let (a, b) = self.select_range();
        self.text.replace_range(a..b, "");
        self.cursor = a;
        for ch in s.chars() {
            if !ch.is_ascii_digit() {
                continue;
            }
            let next = format!("{}{}", &self.text[..self.cursor], ch);
            if next.parse::<u32>().map(|v| v <= 4000).unwrap_or(false) {
                self.text.replace_range(self.cursor..self.cursor, &ch.to_string());
                self.cursor += ch.len_utf8();
            }
        }
    }

    pub fn backspace(&mut self) {
        let (a, b) = self.select_range();
        if a != b {
            self.text.replace_range(a..b, "");
            self.cursor = a;
            return;
        }
        if let Some(prev) = self.text[..self.cursor].char_indices().next_back().map(|(i, _)| i) {
            self.text.replace_range(prev..self.cursor, "");
            self.cursor = prev;
        }
    }

    pub fn delete(&mut self) {
        let (a, b) = self.select_range();
        if a != b {
            self.text.replace_range(a..b, "");
            self.cursor = a;
            return;
        }
        if self.cursor < self.text.len() {
            let next = self.text[self.cursor..].chars().next().map(|c| c.len_utf8()).unwrap_or(0);
            self.text.replace_range(self.cursor..self.cursor + next, "");
        }
    }

    pub fn move_cursor(&mut self, back: bool, extend: bool) {
        if !extend {
            if let Some((a, b)) = self.selection {
                // Leaving a selection puts the caret at its end.
                self.selection = None;
                self.cursor = if back { a.min(b) } else { a.max(b) };
                return;
            }
        } else if self.selection.is_none() {
            self.selection = Some((self.cursor, self.cursor));
        }
        if back {
            if let Some(prev) = self.text[..self.cursor].char_indices().next_back().map(|(i, _)| i) {
                self.cursor = prev;
            }
        } else if self.cursor < self.text.len() {
            self.cursor += self.text[self.cursor..].chars().next().map(|c| c.len_utf8()).unwrap_or(0);
        }
        if let Some((anchor, _)) = self.selection {
            self.selection = Some((anchor, self.cursor));
        }
    }

    pub fn home(&mut self, extend: bool) {
        if extend && self.selection.is_none() {
            self.selection = Some((self.cursor, self.cursor));
        }
        if !extend {
            self.selection = None;
        }
        self.cursor = 0;
    }

    pub fn end(&mut self, extend: bool) {
        if extend && self.selection.is_none() {
            self.selection = Some((self.cursor, self.cursor));
        }
        if !extend {
            self.selection = None;
        }
        self.cursor = self.text.len();
    }

    pub fn select_all(&mut self) {
        self.selection = Some((0, self.text.len()));
        self.cursor = self.text.len();
    }

    pub fn selected_text(&self) -> String {
        self.selection
            .map(|(a, b)| self.text[a.min(b)..a.max(b)].to_string())
            .unwrap_or_default()
    }

    pub fn cut(&mut self) -> String {
        let text = self.selected_text();
        let (a, b) = self.select_range();
        if a != b {
            self.text.replace_range(a..b, "");
            self.cursor = a;
            self.selection = None;
        }
        text
    }

    pub fn paste(&mut self, s: &str) {
        self.insert(s);
    }
}
