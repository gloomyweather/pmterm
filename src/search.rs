use crate::buffer::Buffer;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct SearchState {
    pub pattern: String,
    pub is_forward: bool,
    pub active: bool,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            pattern: String::new(),
            is_forward: true,
            active: false,
        }
    }
}

pub fn find_matches(buffer: &Buffer, pattern: &str) -> Vec<(usize, usize, usize)> {
    let mut matches = Vec::new();
    if pattern.is_empty() {
        return matches;
    }

    // Standard case-insensitive regex compiling
    let Ok(re) = Regex::new(&format!("(?i){}", pattern)) else {
        return matches;
    };

    for line_idx in 0..buffer.rope.len_lines() {
        let line_str = buffer.rope.line(line_idx).to_string();
        for mat in re.find_iter(&line_str) {
            let start_char = byte_to_char_idx(&line_str, mat.start());
            let end_char = byte_to_char_idx(&line_str, mat.end());
            matches.push((line_idx, start_char, end_char));
        }
    }
    matches
}

fn byte_to_char_idx(s: &str, byte_idx: usize) -> usize {
    s.char_indices()
        .take_while(|&(idx, _)| idx < byte_idx)
        .count()
}

pub fn find_next(
    matches: &[(usize, usize, usize)],
    current_line: usize,
    current_col: usize,
    forward: bool,
) -> Option<(usize, usize)> {
    if matches.is_empty() {
        return None;
    }

    if forward {
        for &(line, start, _) in matches {
            if line > current_line || (line == current_line && start > current_col) {
                return Some((line, start));
            }
        }
        let (line, start, _) = matches[0];
        Some((line, start))
    } else {
        for &(line, start, _) in matches.iter().rev() {
            if line < current_line || (line == current_line && start < current_col) {
                return Some((line, start));
            }
        }
        let (line, start, _) = matches[matches.len() - 1];
        Some((line, start))
    }
}
