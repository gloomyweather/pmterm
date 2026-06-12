use crate::buffer::Buffer;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedCommand {
    Save,
    Quit,
    SaveAndQuit,
    ForceQuit,
    Edit(String),
    New,
    SaveAs(String),
    NextBuffer,
    PrevBuffer,
    Help,
    SetNumber(bool),
    Colorscheme(String),
    ReplaceLine { old: String, new: String, global: bool },
    ReplaceFile { old: String, new: String },
    Nohl,
}

pub fn parse_colon_command(input: &str) -> Option<ParsedCommand> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    if input == "w" {
        Some(ParsedCommand::Save)
    } else if input == "q" {
        Some(ParsedCommand::Quit)
    } else if input == "wq" || input == "x" {
        Some(ParsedCommand::SaveAndQuit)
    } else if input == "q!" {
        Some(ParsedCommand::ForceQuit)
    } else if input.starts_with("e ") {
        let filename = input[2..].trim().to_string();
        Some(ParsedCommand::Edit(filename))
    } else if input == "new" {
        Some(ParsedCommand::New)
    } else if input.starts_with("saveas ") {
        let filename = input[7..].trim().to_string();
        Some(ParsedCommand::SaveAs(filename))
    } else if input == "bn" {
        Some(ParsedCommand::NextBuffer)
    } else if input == "bp" {
        Some(ParsedCommand::PrevBuffer)
    } else if input == "help" {
        Some(ParsedCommand::Help)
    } else if input == "set number" {
        Some(ParsedCommand::SetNumber(true))
    } else if input == "set nonumber" {
        Some(ParsedCommand::SetNumber(false))
    } else if input.starts_with("colorscheme ") {
        let theme = input[12..].trim().to_string();
        Some(ParsedCommand::Colorscheme(theme))
    } else if input == "nohl" {
        Some(ParsedCommand::Nohl)
    } else if input.starts_with("%s/") {
        // Form: %s/old/new/g (or %s/old/new/)
        let parts: Vec<&str> = input.split('/').collect();
        if parts.len() >= 3 && parts[0] == "%s" {
            let old = parts[1].to_string();
            let new = parts[2].to_string();
            Some(ParsedCommand::ReplaceFile { old, new })
        } else {
            None
        }
    } else if input.starts_with("s/") {
        // Form: s/old/new/ or s/old/new/g
        let parts: Vec<&str> = input.split('/').collect();
        if parts.len() >= 3 {
            let old = parts[1].to_string();
            let new = parts[2].to_string();
            let global = parts.get(3).map(|&f| f.contains('g')).unwrap_or(false);
            Some(ParsedCommand::ReplaceLine { old, new, global })
        } else {
            None
        }
    } else {
        None
    }
}

pub fn replace_line(buffer: &mut Buffer, old: &str, new: &str, global: bool) -> Result<(), String> {
    if old.is_empty() {
        return Err("Empty pattern".to_string());
    }
    let line_idx = buffer.cursor_line;
    if line_idx >= buffer.rope.len_lines() {
        return Ok(());
    }
    let line_str = buffer.rope.line(line_idx).to_string();

    let re = Regex::new(old).map_err(|e| e.to_string())?;
    let new_line_str = if global {
        re.replace_all(&line_str, new).into_owned()
    } else {
        re.replace(&line_str, new).into_owned()
    };

    if new_line_str != line_str {
        let line_start_char = buffer.rope.line_to_char(line_idx);
        let line_end_char = line_start_char + line_str.chars().count();
        buffer.delete_range(line_start_char, line_end_char);
        
        // Put cursor temporarily back so insert_str inserts in the right place
        let orig_cursor = (buffer.cursor_line, buffer.cursor_col);
        buffer.cursor_line = line_idx;
        buffer.cursor_col = 0;
        buffer.insert_str(&new_line_str);
        buffer.cursor_line = orig_cursor.0;
        buffer.cursor_col = orig_cursor.1.min(buffer.line_len_without_ending(buffer.cursor_line));
    }
    Ok(())
}

pub fn replace_file(buffer: &mut Buffer, old: &str, new: &str) -> Result<(), String> {
    if old.is_empty() {
        return Err("Empty pattern".to_string());
    }
    let re = Regex::new(old).map_err(|e| e.to_string())?;
    let full_text = buffer.rope.to_string();
    let new_text = re.replace_all(&full_text, new).into_owned();

    if new_text != full_text {
        let len = buffer.rope.len_chars();
        buffer.delete_range(0, len);
        
        let orig_cursor = (buffer.cursor_line, buffer.cursor_col);
        buffer.cursor_line = 0;
        buffer.cursor_col = 0;
        buffer.insert_str(&new_text);
        
        buffer.cursor_line = orig_cursor.0.min(buffer.rope.len_lines().saturating_sub(1));
        buffer.cursor_col = orig_cursor.1.min(buffer.line_len_without_ending(buffer.cursor_line));
    }
    Ok(())
}
