use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use anyhow::Result;
use ratatui::style::{Color, Style, Modifier};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub tab_width: usize,
    pub show_line_numbers: bool,
    pub theme: String,
    pub auto_save_interval: u64,
    pub backup_enabled: bool,
    pub syntax_highlight: bool,
    pub preview_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tab_width: 2,
            show_line_numbers: true,
            theme: "dark".to_string(),
            auto_save_interval: 30,
            backup_enabled: true,
            syntax_highlight: true,
            preview_enabled: false,
        }
    }
}

pub fn get_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("mdterm").join("config.toml"))
}

pub fn load_config() -> Config {
    if let Some(path) = get_config_path() {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(config) = toml::from_str(&content) {
                    return config;
                }
            }
        } else {
            // Create directories and write default config
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let default_config = Config::default();
            if let Ok(content) = toml::to_string_pretty(&default_config) {
                let _ = fs::write(&path, content);
            }
        }
    }
    Config::default()
}

pub fn save_config(config: &Config) -> Result<()> {
    if let Some(path) = get_config_path() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(config)?;
        fs::write(&path, content)?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub status_bg: Color,
    pub status_fg: Color,
    pub cursor_line_bg: Color,
    pub line_number_fg: Color,
    pub header1: Style,
    pub header2: Style,
    pub header3: Style,
    pub bold: Style,
    pub italic: Style,
    pub code_inline: Style,
    pub code_block_bg: Color,
    pub link: Style,
    pub image: Style,
    pub checkbox: Style,
    pub checkbox_checked: Style,
    pub hr: Style,
    pub list_bullet: Style,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg: Color::Reset, // Inherit terminal bg
            fg: Color::Gray,
            status_bg: Color::Rgb(40, 44, 52),
            status_fg: Color::Rgb(171, 178, 191),
            cursor_line_bg: Color::Rgb(44, 48, 56),
            line_number_fg: Color::DarkGray,
            header1: Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD),
            header2: Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD),
            header3: Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD),
            bold: Style::default().add_modifier(Modifier::BOLD),
            italic: Style::default().add_modifier(Modifier::ITALIC),
            code_inline: Style::default().fg(Color::Yellow).bg(Color::Rgb(40, 44, 52)),
            code_block_bg: Color::Rgb(30, 30, 30),
            link: Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED),
            image: Style::default().fg(Color::Magenta),
            checkbox: Style::default().fg(Color::Red),
            checkbox_checked: Style::default().fg(Color::Green),
            hr: Style::default().fg(Color::DarkGray),
            list_bullet: Style::default().fg(Color::LightCyan),
        }
    }

    pub fn light() -> Self {
        Self {
            bg: Color::White,
            fg: Color::Black,
            status_bg: Color::Rgb(240, 240, 240),
            status_fg: Color::Rgb(50, 50, 50),
            cursor_line_bg: Color::Rgb(230, 230, 230),
            line_number_fg: Color::Gray,
            header1: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            header2: Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            header3: Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
            bold: Style::default().add_modifier(Modifier::BOLD),
            italic: Style::default().add_modifier(Modifier::ITALIC),
            code_inline: Style::default().fg(Color::DarkGray).bg(Color::Rgb(240, 240, 240)),
            code_block_bg: Color::Rgb(245, 245, 245),
            link: Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED),
            image: Style::default().fg(Color::Magenta),
            checkbox: Style::default().fg(Color::Red),
            checkbox_checked: Style::default().fg(Color::Green),
            hr: Style::default().fg(Color::Gray),
            list_bullet: Style::default().fg(Color::Cyan),
        }
    }

    pub fn get_by_name(name: &str) -> Self {
        match name {
            "light" => Self::light(),
            _ => Self::dark(),
        }
    }
}
