use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, Event as CrosstermEvent, KeyCode, KeyEventKind},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::Terminal;
use std::{io::{stdout, Stdout}, path::PathBuf, panic};

mod buffer;
mod commands;
mod config;
mod editor;
mod preview;
mod render;
mod search;

/// A CLI Markdown editor and viewer.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Files to open.
    #[arg()] // This will collect all positional arguments into a Vec<String>
    files: Vec<String>,

    /// Enable live preview at startup.
    #[arg(long)]
    preview: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let config = config::load_config();

    // Check initial terminal size constraints
    let (cols, rows) = terminal::size()?;
    if cols < 40 || rows < 5 {
        anyhow::bail!("Terminal too small (min 40 cols, 5 rows). Please resize.");
    }

    let mut buffers = Vec::new();
    if args.files.is_empty() {
        buffers.push(buffer::Buffer::new_empty());
    } else {
        for file_path in args.files {
            let path = PathBuf::from(file_path);
            match buffer::Buffer::from_file(path.clone()) {
                Ok(mut b) => {
                    if args.preview {
                        // Ensure preview_enabled is set for each opened buffer if starting with --preview
                        // (Though editor state should manage this globally, setting on buffer might be useful for future features)
                    }
                    buffers.push(b)
                },
                Err(e) => {
                    eprintln!("Error opening file {}: {}", path.display(), e);
                    buffers.push(buffer::Buffer::new_empty()); // Open an empty buffer on error
                }
            }
        }
    }

    let mut editor = editor::Editor::new(buffers, config.clone());
    if args.preview {
        editor.preview_enabled = true;
    }

    // Terminal initialization
    setup_terminal()?;

    let mut terminal = Terminal::new(ratatui::backend::CrosstermBackend::new(stdout()))?;
    let highlighter = render::Highlighter::new();

    // Setup signal handling for graceful exit
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        restore_terminal().expect("Failed to restore terminal");
        original_hook(panic_info);
    }));
    
    let mut last_save = std::time::Instant::now();

    // Main event loop
    loop {
        terminal.draw(|frame| render::draw_ui(frame, &mut editor, &highlighter))?;

        // Event handling with auto-save polling
        let poll_interval = std::time::Duration::from_millis(100);
        if event::poll(poll_interval)? {
            while event::poll(std::time::Duration::from_millis(0))? {
                match event::read()? {
                    CrosstermEvent::Key(key_event) => {
                        if key_event.kind == KeyEventKind::Press {
                            editor.handle_key(key_event);
                        }
                    }
                    CrosstermEvent::Resize(w, h) => {
                        terminal.resize(ratatui::layout::Rect::new(0,0,w,h))?;
                    }
                    _ => {}
                }
            }
        }

        // Auto-save check
        let interval = editor.config.auto_save_interval;
        if interval > 0 && last_save.elapsed().as_secs() >= interval {
            editor.auto_save_modified_buffers();
            last_save = std::time::Instant::now();
            editor.message = Some(("Auto-saved modified buffers".to_string(), false));
        }

        if editor.should_quit {
            break;
        }
    }

    // Terminal cleanup
    restore_terminal()?;

    Ok(())
}

fn setup_terminal() -> Result<()> {
    terminal::enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    Ok(())
}

fn restore_terminal() -> Result<()> {
    terminal::disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
