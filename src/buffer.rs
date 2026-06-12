use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct EditStep {
    pub is_insert: bool,
    pub char_idx: usize,
    pub text: String,
    pub cursor_before: (usize, usize),
    pub cursor_after: (usize, usize),
}

#[derive(Debug, Clone)]
pub struct Buffer {
    pub rope: ropey::Rope,
    pub path: Option<PathBuf>,
    pub modified: bool,
    pub encoding_warning: bool,
    
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub scroll_row: usize,
    pub scroll_col: usize,
    
    pub undo_stack: Vec<EditStep>,
    pub redo_stack: Vec<EditStep>,
}

impl Buffer {
    pub fn new_empty() -> Self {
        Self {
            rope: ropey::Rope::new(),
            path: None,
            modified: false,
            encoding_warning: false,
            cursor_line: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn from_file(path: PathBuf) -> Result<Self> {
        let mut encoding_warning = false;
        let rope = if path.exists() {
            let bytes = std::fs::read(&path)?;
            let text = match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(e) => {
                    encoding_warning = true;
                    String::from_utf8_lossy(e.as_bytes()).into_owned()
                }
            };
            let text_without_bom = if text.starts_with('\u{feff}') {
                text.replacen('\u{feff}', "", 1)
            } else {
                text
            };
            ropey::Rope::from_str(&text_without_bom)
        } else {
            ropey::Rope::new()
        };

        Ok(Self {
            rope,
            path: Some(path),
            modified: false,
            encoding_warning,
            cursor_line: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        })
    }

    pub fn line_len_chars(&self, line_idx: usize) -> usize {
        if line_idx >= self.rope.len_lines() {
            0
        } else {
            self.rope.line(line_idx).len_chars()
        }
    }

    pub fn line_len_without_ending(&self, line_idx: usize) -> usize {
        if line_idx >= self.rope.len_lines() {
            return 0;
        }
        let line = self.rope.line(line_idx);
        let mut len = line.len_chars();
        while len > 0 {
            let last_char = line.char(len - 1);
            if last_char == '\n' || last_char == '\r' {
                len -= 1;
            } else {
                break;
            }
        }
        len
    }

    pub fn line_col_to_char_idx(&self, line: usize, col: usize) -> usize {
        if line >= self.rope.len_lines() {
            return self.rope.len_chars();
        }
        let line_start = self.rope.line_to_char(line);
        let line_len = self.line_len_without_ending(line);
        let col = col.min(line_len);
        line_start + col
    }

    pub fn char_idx_to_line_col(&self, char_idx: usize) -> (usize, usize) {
        let len = self.rope.len_chars();
        let char_idx = char_idx.min(len);
        let line = self.rope.char_to_line(char_idx);
        let line_start = self.rope.line_to_char(line);
        let col = char_idx - line_start;
        (line, col)
    }

    pub fn insert_char(&mut self, ch: char) {
        let cursor_before = (self.cursor_line, self.cursor_col);
        let char_idx = self.line_col_to_char_idx(self.cursor_line, self.cursor_col);
        self.rope.insert(char_idx, &ch.to_string());
        
        if ch == '\n' {
            self.cursor_line += 1;
            self.cursor_col = 0;
        } else {
            self.cursor_col += 1;
        }
        let cursor_after = (self.cursor_line, self.cursor_col);
        self.modified = true;

        let mut merged = false;
        if ch != '\n' && ch != ' ' && ch != '\t' {
            if let Some(last) = self.undo_stack.last_mut() {
                if last.is_insert 
                    && last.char_idx + last.text.chars().count() == char_idx 
                    && !last.text.ends_with('\n') 
                    && !last.text.ends_with(' ')
                    && !last.text.ends_with('\t')
                {
                    last.text.push(ch);
                    last.cursor_after = cursor_after;
                    merged = true;
                }
            }
        }

        if !merged {
            self.undo_stack.push(EditStep {
                is_insert: true,
                char_idx,
                text: ch.to_string(),
                cursor_before,
                cursor_after,
            });
        }
        self.redo_stack.clear();
    }

    pub fn insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let cursor_before = (self.cursor_line, self.cursor_col);
        let char_idx = self.line_col_to_char_idx(self.cursor_line, self.cursor_col);
        self.rope.insert(char_idx, s);
        
        let lines_added = s.matches('\n').count();
        if lines_added > 0 {
            self.cursor_line += lines_added;
            if let Some(last_line) = s.split('\n').last() {
                self.cursor_col = last_line.chars().count();
            }
        } else {
            self.cursor_col += s.chars().count();
        }
        let cursor_after = (self.cursor_line, self.cursor_col);
        self.modified = true;

        self.undo_stack.push(EditStep {
            is_insert: true,
            char_idx,
            text: s.to_string(),
            cursor_before,
            cursor_after,
        });
        self.redo_stack.clear();
    }

    pub fn backspace(&mut self) {
        if self.cursor_col == 0 && self.cursor_line == 0 {
            return;
        }
        let cursor_before = (self.cursor_line, self.cursor_col);
        let char_idx = self.line_col_to_char_idx(self.cursor_line, self.cursor_col);
        if char_idx == 0 {
            return;
        }
        let delete_idx = char_idx - 1;
        let deleted = self.rope.slice(delete_idx..char_idx).to_string();
        self.rope.remove(delete_idx..char_idx);

        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else {
            self.cursor_line -= 1;
            self.cursor_col = self.line_len_without_ending(self.cursor_line);
        }
        let cursor_after = (self.cursor_line, self.cursor_col);
        self.modified = true;

        let mut merged = false;
        if deleted != "\n" && deleted != " " && deleted != "\t" {
            if let Some(last) = self.undo_stack.last_mut() {
                if !last.is_insert 
                    && last.char_idx == char_idx 
                    && !last.text.starts_with('\n') 
                    && !last.text.starts_with(' ')
                    && !last.text.starts_with('\t')
                {
                    last.text = format!("{}{}", deleted, last.text);
                    last.char_idx = delete_idx;
                    last.cursor_after = cursor_after;
                    merged = true;
                }
            }
        }

        if !merged {
            self.undo_stack.push(EditStep {
                is_insert: false,
                char_idx: delete_idx,
                text: deleted,
                cursor_before,
                cursor_after,
            });
        }
        self.redo_stack.clear();
    }

    pub fn delete_char(&mut self) {
        let char_idx = self.line_col_to_char_idx(self.cursor_line, self.cursor_col);
        if char_idx >= self.rope.len_chars() {
            return;
        }
        let deleted = self.rope.slice(char_idx..char_idx + 1).to_string();
        self.rope.remove(char_idx..char_idx + 1);
        self.modified = true;

        self.undo_stack.push(EditStep {
            is_insert: false,
            char_idx,
            text: deleted,
            cursor_before: (self.cursor_line, self.cursor_col),
            cursor_after: (self.cursor_line, self.cursor_col),
        });
        self.redo_stack.clear();
    }

    pub fn delete_range(&mut self, start_char: usize, end_char: usize) {
        if start_char >= end_char || start_char >= self.rope.len_chars() {
            return;
        }
        let end_char = end_char.min(self.rope.len_chars());
        let deleted = self.rope.slice(start_char..end_char).to_string();
        self.rope.remove(start_char..end_char);
        self.modified = true;

        let (line_b, col_b) = self.char_idx_to_line_col(start_char);
        self.undo_stack.push(EditStep {
            is_insert: false,
            char_idx: start_char,
            text: deleted,
            cursor_before: (self.cursor_line, self.cursor_col),
            cursor_after: (line_b, col_b),
        });
        self.redo_stack.clear();
        self.cursor_line = line_b;
        self.cursor_col = col_b;
    }

    pub fn undo(&mut self) -> Option<(usize, usize)> {
        if let Some(step) = self.undo_stack.pop() {
            if step.is_insert {
                let start = step.char_idx;
                let end = step.char_idx + step.text.chars().count();
                self.rope.remove(start..end);
            } else {
                self.rope.insert(step.char_idx, &step.text);
            }
            self.modified = true;
            let pos = step.cursor_before;
            self.cursor_line = pos.0;
            self.cursor_col = pos.1;
            self.redo_stack.push(step);
            Some(pos)
        } else {
            None
        }
    }

    pub fn redo(&mut self) -> Option<(usize, usize)> {
        if let Some(step) = self.redo_stack.pop() {
            if step.is_insert {
                self.rope.insert(step.char_idx, &step.text);
            } else {
                let start = step.char_idx;
                let end = step.char_idx + step.text.chars().count();
                self.rope.remove(start..end);
            }
            self.modified = true;
            let pos = step.cursor_after;
            self.cursor_line = pos.0;
            self.cursor_col = pos.1;
            self.undo_stack.push(step);
            Some(pos)
        } else {
            None
        }
    }

    pub fn save(&mut self, backup_enabled: bool) -> Result<()> {
        let Some(ref path) = self.path else {
            anyhow::bail!("No file path is associated with this buffer. Use :saveas <filename>");
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if backup_enabled && path.exists() {
            let mut backup_path = path.clone();
            let mut filename = path.file_name().unwrap_or_default().to_os_string();
            filename.push(".bak");
            backup_path.set_file_name(filename);
            let _ = std::fs::copy(path, backup_path);
        }

        let content = self.rope.to_string();
        std::fs::write(path, content)?;
        self.modified = false;
        Ok(())
    }
}
