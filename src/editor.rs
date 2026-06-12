use crate::buffer::Buffer;
use crate::config::Config;
use crate::search::{SearchState, find_matches, find_next};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandType {
    Colon,
    Slash,
    Question,
}

pub struct Editor {
    pub buffers: Vec<Buffer>,
    pub current_buffer_idx: usize,
    pub mode: Mode,
    pub preview_enabled: bool,
    pub clipboard: String,

    pub command_buffer: String,
    pub command_type: Option<CommandType>,
    pub search_state: SearchState,

    pub visual_anchor: Option<(usize, usize)>,

    pub message: Option<(String, bool)>,
    pub show_help: bool,
    pub config: Config,
    pub should_quit: bool,

    pub pending_key: Option<char>,
}

#[derive(PartialEq, Eq)]
enum CharClass {
    Whitespace,
    Punctuation,
    Word,
}

fn char_class(ch: char, uppercase: bool) -> CharClass {
    if ch.is_whitespace() {
        CharClass::Whitespace
    } else if uppercase {
        CharClass::Word
    } else if ch.is_alphanumeric() || ch == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

impl Editor {
    pub fn new(buffers: Vec<Buffer>, config: Config) -> Self {
        let preview_enabled = config.preview_enabled;
        Self {
            buffers,
            current_buffer_idx: 0,
            mode: Mode::Normal,
            preview_enabled,
            clipboard: String::new(),
            command_buffer: String::new(),
            command_type: None,
            search_state: SearchState::new(),
            visual_anchor: None,
            message: None,
            show_help: false,
            config,
            should_quit: false,
            pending_key: None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // Any key clears help overlay
        if self.show_help {
            self.show_help = false;
            return;
        }

        // Global keys in any mode
        if key.modifiers == KeyModifiers::CONTROL {
            match key.code {
                KeyCode::Char('s') => {
                    let buffer = &mut self.buffers[self.current_buffer_idx];
                    match buffer.save(self.config.backup_enabled) {
                        Ok(_) => self.message = Some(("File saved successfully".to_string(), false)),
                        Err(e) => self.message = Some((format!("Error saving file: {}", e), true)),
                    }
                    return;
                }
                KeyCode::Char('p') => {
                    self.preview_enabled = !self.preview_enabled;
                    return;
                }
                _ => {}
            }
        }

        match self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Insert => self.handle_insert_key(key),
            Mode::Visual => self.handle_visual_key(key),
            Mode::Command => self.handle_command_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        // Buffer navigation
        if key.modifiers == KeyModifiers::CONTROL {
            match key.code {
                KeyCode::Tab => {
                    self.current_buffer_idx = (self.current_buffer_idx + 1) % self.buffers.len();
                    return;
                }
                _ => {}
            }
        } else if key.modifiers == (KeyModifiers::CONTROL | KeyModifiers::SHIFT) {
            match key.code {
                KeyCode::Tab | KeyCode::BackTab => {
                    self.current_buffer_idx = (self.current_buffer_idx + self.buffers.len() - 1) % self.buffers.len();
                    return;
                }
                _ => {}
            }
        }

        // Handle double-key commands (dd, yy, gg)
        if let Some(pending) = self.pending_key {
            self.pending_key = None;
            match (pending, key.code) {
                ('d', KeyCode::Char('d')) => {
                    self.delete_current_line();
                    return;
                }
                ('y', KeyCode::Char('y')) => {
                    self.yank_current_line();
                    return;
                }
                ('g', KeyCode::Char('g')) => {
                    let buffer = &mut self.buffers[self.current_buffer_idx];
                    buffer.cursor_line = 0;
                    buffer.cursor_col = 0;
                    return;
                }
                _ => {}
            }
        }

        let buffer = &mut self.buffers[self.current_buffer_idx];

        match key.code {
            // Movement keys
            KeyCode::Char('h') | KeyCode::Left => {
                if buffer.cursor_col > 0 {
                    buffer.cursor_col -= 1;
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                let len = buffer.line_len_without_ending(buffer.cursor_line);
                if buffer.cursor_col < len.saturating_sub(1) {
                    buffer.cursor_col += 1;
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if buffer.cursor_line + 1 < buffer.rope.len_lines() {
                    buffer.cursor_line += 1;
                    buffer.cursor_col = buffer.cursor_col.min(buffer.line_len_without_ending(buffer.cursor_line).saturating_sub(1));
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if buffer.cursor_line > 0 {
                    buffer.cursor_line -= 1;
                    buffer.cursor_col = buffer.cursor_col.min(buffer.line_len_without_ending(buffer.cursor_line).saturating_sub(1));
                }
            }

            // Word movements
            KeyCode::Char('w') => self.move_word_forward(false),
            KeyCode::Char('W') => self.move_word_forward(true),
            KeyCode::Char('b') => self.move_word_backward(false),
            KeyCode::Char('B') => self.move_word_backward(true),

            // Start/End line movements
            KeyCode::Char('0') => {
                buffer.cursor_col = 0;
            }
            KeyCode::Char('^') => {
                let line_str = buffer.rope.line(buffer.cursor_line).to_string();
                let leading_ws = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                buffer.cursor_col = leading_ws;
            }
            KeyCode::Char('$') => {
                buffer.cursor_col = buffer.line_len_without_ending(buffer.cursor_line).saturating_sub(1);
            }

            // Document movements
            KeyCode::Char('G') => {
                buffer.cursor_line = buffer.rope.len_lines().saturating_sub(1);
                buffer.cursor_col = 0;
            }
            KeyCode::Char('g') => {
                self.pending_key = Some('g');
            }

            // Scrolling
            KeyCode::Char('d') if key.modifiers == KeyModifiers::CONTROL => {
                // Scroll half page down
                buffer.cursor_line = (buffer.cursor_line + 15).min(buffer.rope.len_lines().saturating_sub(1));
            }
            KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
                // Scroll half page up
                buffer.cursor_line = buffer.cursor_line.saturating_sub(15);
            }
            KeyCode::Char('f') if key.modifiers == KeyModifiers::CONTROL => {
                // Scroll full page down
                buffer.cursor_line = (buffer.cursor_line + 30).min(buffer.rope.len_lines().saturating_sub(1));
            }
            KeyCode::Char('b') if key.modifiers == KeyModifiers::CONTROL => {
                // Scroll full page up
                buffer.cursor_line = buffer.cursor_line.saturating_sub(30);
            }

            KeyCode::Char('z') => {
                // Centering scrolling behavior
                self.pending_key = Some('z');
            }
            KeyCode::Char('z') if self.pending_key == Some('z') => {
                self.pending_key = None;
                // zz action: make cursor line vertical center
                buffer.scroll_row = buffer.cursor_line.saturating_sub(15);
            }

            // Mode changes
            KeyCode::Char('i') => {
                self.mode = Mode::Insert;
                self.clamp_cursor();
            }
            KeyCode::Char('a') => {
                self.mode = Mode::Insert;
                let len = buffer.line_len_without_ending(buffer.cursor_line);
                if len > 0 {
                    buffer.cursor_col = (buffer.cursor_col + 1).min(len);
                }
            }
            KeyCode::Char('I') => {
                self.mode = Mode::Insert;
                let line_str = buffer.rope.line(buffer.cursor_line).to_string();
                let leading_ws = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                buffer.cursor_col = leading_ws;
            }
            KeyCode::Char('A') => {
                self.mode = Mode::Insert;
                buffer.cursor_col = buffer.line_len_without_ending(buffer.cursor_line);
            }
            KeyCode::Char('o') => {
                self.mode = Mode::Insert;
                buffer.cursor_line += 1;
                buffer.cursor_col = 0;
                let char_idx = buffer.line_col_to_char_idx(buffer.cursor_line, 0);
                buffer.rope.insert(char_idx, "\n");
                buffer.modified = true;
            }
            KeyCode::Char('O') => {
                self.mode = Mode::Insert;
                buffer.cursor_col = 0;
                let char_idx = buffer.line_col_to_char_idx(buffer.cursor_line, 0);
                buffer.rope.insert(char_idx, "\n");
                buffer.modified = true;
            }
            KeyCode::Char('v') => {
                self.mode = Mode::Visual;
                self.visual_anchor = Some((buffer.cursor_line, buffer.cursor_col));
            }

            // Colon Command Modes
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.command_type = Some(CommandType::Colon);
                self.command_buffer.clear();
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Command;
                self.command_type = Some(CommandType::Slash);
                self.command_buffer.clear();
            }
            KeyCode::Char('?') => {
                self.mode = Mode::Command;
                self.command_type = Some(CommandType::Question);
                self.command_buffer.clear();
            }

            // Search Navigation
            KeyCode::Char('n') => {
                let matches = find_matches(buffer, &self.search_state.pattern);
                if let Some((l, c)) = find_next(&matches, buffer.cursor_line, buffer.cursor_col, self.search_state.is_forward) {
                    buffer.cursor_line = l;
                    buffer.cursor_col = c;
                }
            }
            KeyCode::Char('N') => {
                let matches = find_matches(buffer, &self.search_state.pattern);
                if let Some((l, c)) = find_next(&matches, buffer.cursor_line, buffer.cursor_col, !self.search_state.is_forward) {
                    buffer.cursor_line = l;
                    buffer.cursor_col = c;
                }
            }

            // Deletions & Clipboard
            KeyCode::Char('x') | KeyCode::Delete => {
                buffer.delete_char();
                self.clamp_cursor();
            }
            KeyCode::Char('d') => {
                self.pending_key = Some('d');
            }
            KeyCode::Char('y') => {
                self.pending_key = Some('y');
            }
            KeyCode::Char('p') => {
                self.paste(true);
            }
            KeyCode::Char('P') => {
                self.paste(false);
            }

            // Undo / Redo
            KeyCode::Char('u') => {
                if let Some((l, c)) = buffer.undo() {
                    buffer.cursor_line = l;
                    buffer.cursor_col = c;
                    self.message = Some(("Undo action performed".to_string(), false));
                }
            }
            KeyCode::Char('R') if key.modifiers == KeyModifiers::CONTROL => {
                if let Some((l, c)) = buffer.redo() {
                    buffer.cursor_line = l;
                    buffer.cursor_col = c;
                    self.message = Some(("Redo action performed".to_string(), false));
                }
            }

            // Quick quit
            KeyCode::Char('q') => {
                let is_mod = buffer.modified;
                if is_mod {
                    self.message = Some(("No write since last change (add ! to override)".to_string(), true));
                } else if self.buffers.len() > 1 {
                    self.buffers.remove(self.current_buffer_idx);
                    self.current_buffer_idx = self.current_buffer_idx.min(self.buffers.len() - 1);
                } else {
                    self.should_quit = true;
                }
            }
            KeyCode::Char('Z') => {
                self.pending_key = Some('Z');
            }
            KeyCode::Char('Z') if self.pending_key == Some('Z') => {
                self.pending_key = None;
                // ZZ save and quit
                let is_error = match buffer.save(self.config.backup_enabled) {
                    Ok(_) => false,
                    Err(e) => {
                        self.message = Some((format!("Error saving file: {}", e), true));
                        true
                    }
                };
                if !is_error {
                    self.should_quit = true;
                }
            }

            _ => {}
        }
    }

    fn handle_insert_key(&mut self, key: KeyEvent) {
        let buffer = &mut self.buffers[self.current_buffer_idx];

        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.clamp_cursor();
            }
            KeyCode::Enter => {
                let prev_line_idx = buffer.cursor_line;
                let line_str = buffer.rope.line(prev_line_idx).to_string();
                let indent: String = line_str.chars().take_while(|&c| c == ' ' || c == '\t').collect();
                
                buffer.insert_char('\n');
                if !indent.is_empty() {
                    buffer.insert_str(&indent);
                }
            }
            KeyCode::Tab => {
                let spaces = " ".repeat(self.config.tab_width);
                buffer.insert_str(&spaces);
            }
            KeyCode::Backspace => {
                buffer.backspace();
            }
            KeyCode::Delete => {
                buffer.delete_char();
            }
            KeyCode::Left => {
                if buffer.cursor_col > 0 {
                    buffer.cursor_col -= 1;
                }
            }
            KeyCode::Right => {
                let len = buffer.line_len_without_ending(buffer.cursor_line);
                if buffer.cursor_col < len {
                    buffer.cursor_col += 1;
                }
            }
            KeyCode::Down => {
                if buffer.cursor_line + 1 < buffer.rope.len_lines() {
                    buffer.cursor_line += 1;
                    buffer.cursor_col = buffer.cursor_col.min(buffer.line_len_without_ending(buffer.cursor_line));
                }
            }
            KeyCode::Up => {
                if buffer.cursor_line > 0 {
                    buffer.cursor_line -= 1;
                    buffer.cursor_col = buffer.cursor_col.min(buffer.line_len_without_ending(buffer.cursor_line));
                }
            }
            KeyCode::Home => {
                buffer.cursor_col = 0;
            }
            KeyCode::End => {
                buffer.cursor_col = buffer.line_len_without_ending(buffer.cursor_line);
            }

            // Word deletions and inline helpers
            KeyCode::Char('w') if key.modifiers == KeyModifiers::CONTROL => {
                self.delete_prev_word_insert();
            }
            KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
                self.delete_to_start_insert();
            }

            KeyCode::Char(ch) => {
                buffer.insert_char(ch);
            }
            _ => {}
        }
    }

    fn handle_visual_key(&mut self, key: KeyEvent) {
        let buffer = &mut self.buffers[self.current_buffer_idx];

        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.visual_anchor = None;
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if buffer.cursor_col > 0 {
                    buffer.cursor_col -= 1;
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                let len = buffer.line_len_without_ending(buffer.cursor_line);
                if buffer.cursor_col < len {
                    buffer.cursor_col += 1;
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if buffer.cursor_line + 1 < buffer.rope.len_lines() {
                    buffer.cursor_line += 1;
                    buffer.cursor_col = buffer.cursor_col.min(buffer.line_len_without_ending(buffer.cursor_line));
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if buffer.cursor_line > 0 {
                    buffer.cursor_line -= 1;
                    buffer.cursor_col = buffer.cursor_col.min(buffer.line_len_without_ending(buffer.cursor_line));
                }
            }

            KeyCode::Char('y') => {
                self.yank_selection();
            }
            KeyCode::Char('d') | KeyCode::Char('x') => {
                self.delete_selection(key.code == KeyCode::Char('x'));
            }
            _ => {}
        }
    }

    fn handle_command_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.command_buffer.clear();
                self.command_type = None;
            }
            KeyCode::Enter => {
                let cmd_str = self.command_buffer.clone();
                let cmd_type = self.command_type.take();
                self.mode = Mode::Normal;
                self.command_buffer.clear();

                if let Some(t) = cmd_type {
                    match t {
                        CommandType::Colon => {
                            if let Some(cmd) = crate::commands::parse_colon_command(&cmd_str) {
                                self.execute_colon_cmd(cmd);
                            } else {
                                self.message = Some(("Unknown command".to_string(), true));
                            }
                        }
                        CommandType::Slash | CommandType::Question => {
                            let buffer = &mut self.buffers[self.current_buffer_idx];
                            self.search_state.pattern = cmd_str;
                            self.search_state.active = true;
                            self.search_state.is_forward = t == CommandType::Slash;
                            
                            let matches = find_matches(buffer, &self.search_state.pattern);
                            if let Some((l, c)) = find_next(&matches, buffer.cursor_line, buffer.cursor_col, self.search_state.is_forward) {
                                buffer.cursor_line = l;
                                buffer.cursor_col = c;
                            } else {
                                self.message = Some(("Pattern not found".to_string(), true));
                            }
                        }
                    }
                }
            }
            KeyCode::Backspace => {
                if self.command_buffer.is_empty() {
                    self.mode = Mode::Normal;
                    self.command_type = None;
                } else {
                    self.command_buffer.pop();
                }
            }
            KeyCode::Char(ch) => {
                self.command_buffer.push(ch);
            }
            _ => {}
        }
    }

    fn clamp_cursor(&mut self) {
        let buffer = &mut self.buffers[self.current_buffer_idx];
        if buffer.cursor_line >= buffer.rope.len_lines() {
            buffer.cursor_line = buffer.rope.len_lines().saturating_sub(1);
        }
        let len = buffer.line_len_without_ending(buffer.cursor_line);
        if self.mode == Mode::Insert {
            buffer.cursor_col = buffer.cursor_col.min(len);
        } else {
            buffer.cursor_col = buffer.cursor_col.min(len.saturating_sub(1));
        }
    }

    fn move_word_forward(&mut self, uppercase: bool) {
        let buffer = &mut self.buffers[self.current_buffer_idx];
        let mut char_idx = buffer.line_col_to_char_idx(buffer.cursor_line, buffer.cursor_col);
        let total_chars = buffer.rope.len_chars();
        if char_idx >= total_chars {
            return;
        }

        let mut iter = buffer.rope.chars_at(char_idx);
        if let Some(first_char) = iter.next() {
            char_idx += 1;
            let first_class = char_class(first_char, uppercase);

            if first_class != CharClass::Whitespace {
                while let Some(ch) = iter.next() {
                    if char_class(ch, uppercase) == first_class {
                        char_idx += 1;
                    } else {
                        break;
                    }
                }
            }
        }

        let mut iter = buffer.rope.chars_at(char_idx);
        while let Some(ch) = iter.next() {
            if char_class(ch, uppercase) == CharClass::Whitespace {
                char_idx += 1;
            } else {
                break;
            }
        }

        let (line, col) = buffer.char_idx_to_line_col(char_idx);
        buffer.cursor_line = line;
        buffer.cursor_col = col;
    }

    fn move_word_backward(&mut self, uppercase: bool) {
        let buffer = &mut self.buffers[self.current_buffer_idx];
        let mut char_idx = buffer.line_col_to_char_idx(buffer.cursor_line, buffer.cursor_col);
        if char_idx == 0 {
            return;
        }

        let mut iter = buffer.rope.chars_at(char_idx);
        while char_idx > 0 {
            if let Some(ch) = iter.prev() {
                if char_class(ch, uppercase) == CharClass::Whitespace {
                    char_idx -= 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        if char_idx == 0 {
            buffer.cursor_line = 0;
            buffer.cursor_col = 0;
            return;
        }

        let mut iter = buffer.rope.chars_at(char_idx);
        if let Some(first_char) = iter.prev() {
            let first_class = char_class(first_char, uppercase);
            if first_class != CharClass::Whitespace {
                while char_idx > 0 {
                    if let Some(ch) = iter.prev() {
                        if char_class(ch, uppercase) == first_class {
                            char_idx -= 1;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
        }

        let (line, col) = buffer.char_idx_to_line_col(char_idx);
        buffer.cursor_line = line;
        buffer.cursor_col = col;
    }

    fn delete_current_line(&mut self) {
        let buffer = &mut self.buffers[self.current_buffer_idx];
        let line_idx = buffer.cursor_line;
        if line_idx >= buffer.rope.len_lines() {
            return;
        }

        let line_start = buffer.rope.line_to_char(line_idx);
        let next_line_start = if line_idx + 1 < buffer.rope.len_lines() {
            buffer.rope.line_to_char(line_idx + 1)
        } else {
            buffer.rope.len_chars()
        };

        let line_text = buffer.rope.slice(line_start..next_line_start).to_string();
        self.clipboard = line_text;

        buffer.delete_range(line_start, next_line_start);

        if buffer.cursor_line >= buffer.rope.len_lines() && buffer.rope.len_lines() > 0 {
            buffer.cursor_line = buffer.rope.len_lines() - 1;
        }
        buffer.cursor_col = 0;
    }

    fn yank_current_line(&mut self) {
        let buffer = &self.buffers[self.current_buffer_idx];
        let line_idx = buffer.cursor_line;
        if line_idx >= buffer.rope.len_lines() {
            return;
        }
        let line_start = buffer.rope.line_to_char(line_idx);
        let next_line_start = if line_idx + 1 < buffer.rope.len_lines() {
            buffer.rope.line_to_char(line_idx + 1)
        } else {
            buffer.rope.len_chars()
        };
        self.clipboard = buffer.rope.slice(line_start..next_line_start).to_string();
        self.message = Some(("Yanked 1 line".to_string(), false));
    }

    fn paste(&mut self, after: bool) {
        if self.clipboard.is_empty() {
            return;
        }
        let buffer = &mut self.buffers[self.current_buffer_idx];
        if self.clipboard.ends_with('\n') {
            if after {
                let target_line = buffer.cursor_line + 1;
                let orig_cursor = (buffer.cursor_line, buffer.cursor_col);
                buffer.cursor_line = target_line.min(buffer.rope.len_lines());
                buffer.cursor_col = 0;
                buffer.insert_str(&self.clipboard);
                buffer.cursor_line = orig_cursor.0 + 1;
                buffer.cursor_col = 0;
            } else {
                let target_line = buffer.cursor_line;
                let orig_cursor = (buffer.cursor_line, buffer.cursor_col);
                buffer.cursor_line = target_line;
                buffer.cursor_col = 0;
                buffer.insert_str(&self.clipboard);
                buffer.cursor_line = orig_cursor.0;
                buffer.cursor_col = 0;
            }
        } else {
            if after && buffer.line_len_without_ending(buffer.cursor_line) > 0 {
                buffer.cursor_col += 1;
            }
            buffer.insert_str(&self.clipboard);
        }
    }

    fn yank_selection(&mut self) {
        let buffer = &mut self.buffers[self.current_buffer_idx];
        let Some(anchor) = self.visual_anchor else { return; };
        let cursor = (buffer.cursor_line, buffer.cursor_col);

        let start_char = buffer.line_col_to_char_idx(anchor.0, anchor.1)
            .min(buffer.line_col_to_char_idx(cursor.0, cursor.1));
        let end_char = buffer.line_col_to_char_idx(anchor.0, anchor.1)
            .max(buffer.line_col_to_char_idx(cursor.0, cursor.1));

        if start_char < end_char {
            self.clipboard = buffer.rope.slice(start_char..end_char).to_string();
            self.message = Some((format!("Yanked {} characters", end_char - start_char), false));
        }

        self.visual_anchor = None;
        self.mode = Mode::Normal;
    }

    fn delete_selection(&mut self, is_cut: bool) {
        let buffer = &mut self.buffers[self.current_buffer_idx];
        let Some(anchor) = self.visual_anchor else { return; };
        let cursor = (buffer.cursor_line, buffer.cursor_col);

        let start_char = buffer.line_col_to_char_idx(anchor.0, anchor.1)
            .min(buffer.line_col_to_char_idx(cursor.0, cursor.1));
        let end_char = buffer.line_col_to_char_idx(anchor.0, anchor.1)
            .max(buffer.line_col_to_char_idx(cursor.0, cursor.1));

        if start_char < end_char {
            let deleted = buffer.rope.slice(start_char..end_char).to_string();
            if is_cut {
                self.clipboard = deleted;
            }
            buffer.delete_range(start_char, end_char);
        }

        self.visual_anchor = None;
        self.mode = Mode::Normal;
    }

    fn delete_prev_word_insert(&mut self) {
        let buffer = &mut self.buffers[self.current_buffer_idx];
        let orig_col = buffer.cursor_col;
        if orig_col == 0 {
            return;
        }

        let line_idx = buffer.cursor_line;
        let line_str = buffer.rope.line(line_idx).to_string();
        let chars: Vec<char> = line_str.chars().collect();

        let mut i = orig_col.min(chars.len());
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        if i > 0 {
            let first_alphanumeric = chars[i - 1].is_alphanumeric();
            while i > 0 && chars[i - 1].is_alphanumeric() == first_alphanumeric && !chars[i - 1].is_whitespace() {
                i -= 1;
            }
        }

        let start_char = buffer.line_col_to_char_idx(line_idx, i);
        let end_char = buffer.line_col_to_char_idx(line_idx, orig_col);
        buffer.delete_range(start_char, end_char);
    }

    fn delete_to_start_insert(&mut self) {
        let buffer = &mut self.buffers[self.current_buffer_idx];
        let orig_col = buffer.cursor_col;
        if orig_col == 0 {
            return;
        }
        let start_char = buffer.line_col_to_char_idx(buffer.cursor_line, 0);
        let end_char = buffer.line_col_to_char_idx(buffer.cursor_line, orig_col);
        buffer.delete_range(start_char, end_char);
    }

    pub fn auto_save_modified_buffers(&mut self) {
        for buffer in &mut self.buffers {
            if buffer.modified && buffer.path.is_some() {
                let _ = buffer.save(self.config.backup_enabled);
            }
        }
    }

    fn execute_colon_cmd(&mut self, cmd: crate::commands::ParsedCommand) {
        use crate::commands::ParsedCommand;
        self.message = None;
        match cmd {
            ParsedCommand::Save => {
                let buffer = &mut self.buffers[self.current_buffer_idx];
                match buffer.save(self.config.backup_enabled) {
                    Ok(_) => self.message = Some(("File saved successfully".to_string(), false)),
                    Err(e) => self.message = Some((format!("Error saving file: {}", e), true)),
                }
            }
            ParsedCommand::Quit => {
                let buffer = &self.buffers[self.current_buffer_idx];
                if buffer.modified {
                    self.message = Some(("No write since last change (add ! to override)".to_string(), true));
                } else if self.buffers.len() > 1 {
                    self.buffers.remove(self.current_buffer_idx);
                    self.current_buffer_idx = self.current_buffer_idx.min(self.buffers.len() - 1);
                } else {
                    self.should_quit = true;
                }
            }
            ParsedCommand::SaveAndQuit => {
                let buffer = &mut self.buffers[self.current_buffer_idx];
                match buffer.save(self.config.backup_enabled) {
                    Ok(_) => {
                        if self.buffers.len() > 1 {
                            self.buffers.remove(self.current_buffer_idx);
                            self.current_buffer_idx = self.current_buffer_idx.min(self.buffers.len() - 1);
                        } else {
                            self.should_quit = true;
                        }
                    }
                    Err(e) => self.message = Some((format!("Error saving file: {}", e), true)),
                }
            }
            ParsedCommand::ForceQuit => {
                if self.buffers.len() > 1 {
                    self.buffers.remove(self.current_buffer_idx);
                    self.current_buffer_idx = self.current_buffer_idx.min(self.buffers.len() - 1);
                } else {
                    self.should_quit = true;
                }
            }
            ParsedCommand::Edit(filename) => {
                let path = PathBuf::from(&filename);
                match Buffer::from_file(path) {
                    Ok(b) => {
                        self.buffers[self.current_buffer_idx] = b;
                        self.message = Some((format!("Opened {}", filename), false));
                    }
                    Err(e) => self.message = Some((format!("Error opening file: {}", e), true)),
                }
            }
            ParsedCommand::New => {
                self.buffers.push(Buffer::new_empty());
                self.current_buffer_idx = self.buffers.len() - 1;
                self.message = Some(("Opened new empty buffer".to_string(), false));
            }
            ParsedCommand::SaveAs(filename) => {
                let path = PathBuf::from(&filename);
                let buffer = &mut self.buffers[self.current_buffer_idx];
                buffer.path = Some(path);
                match buffer.save(self.config.backup_enabled) {
                    Ok(_) => self.message = Some((format!("Saved as {}", filename), false)),
                    Err(e) => self.message = Some((format!("Error saving file: {}", e), true)),
                }
            }
            ParsedCommand::NextBuffer => {
                self.current_buffer_idx = (self.current_buffer_idx + 1) % self.buffers.len();
            }
            ParsedCommand::PrevBuffer => {
                self.current_buffer_idx = (self.current_buffer_idx + self.buffers.len() - 1) % self.buffers.len();
            }
            ParsedCommand::Help => {
                self.show_help = true;
            }
            ParsedCommand::SetNumber(val) => {
                self.config.show_line_numbers = val;
                let _ = crate::config::save_config(&self.config);
            }
            ParsedCommand::Colorscheme(theme) => {
                self.config.theme = theme;
                let _ = crate::config::save_config(&self.config);
            }
            ParsedCommand::Nohl => {
                self.search_state.active = false;
            }
            ParsedCommand::ReplaceLine { old, new, global } => {
                let buffer = &mut self.buffers[self.current_buffer_idx];
                match crate::commands::replace_line(buffer, &old, &new, global) {
                    Ok(_) => self.message = Some(("Substitution done".to_string(), false)),
                    Err(e) => self.message = Some((format!("Substitution error: {}", e), true)),
                }
            }
            ParsedCommand::ReplaceFile { old, new } => {
                let buffer = &mut self.buffers[self.current_buffer_idx];
                match crate::commands::replace_file(buffer, &old, &new) {
                    Ok(_) => self.message = Some(("Global substitution done".to_string(), false)),
                    Err(e) => self.message = Some((format!("Substitution error: {}", e), true)),
                }
            }
        }
    }
}
