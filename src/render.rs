use crate::config::Theme;
use crate::editor::{Editor, Mode};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color as TuiColor, Modifier as TuiModifier, Style as TuiStyle};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use syntect::easy::HighlightLines;
use syntect::highlighting::{ThemeSet, FontStyle};
use syntect::parsing::SyntaxSet;
use pulldown_cmark::{Parser, Event, Tag, CodeBlockKind, HeadingLevel, Alignment};

#[derive(Debug, Clone)]
struct StyledSpan {
    text: String,
    style: TuiStyle,
}

#[derive(Debug, Clone)]
struct StyledWord {
    text: String,
    style: TuiStyle,
}

#[derive(Clone, Debug)]
struct ListState {
    is_ordered: bool,
    index: u64,
    item_has_task_marker: bool,
    item_checked: bool,
}

#[derive(Debug, Clone)]
struct TableCell {
    spans: Vec<StyledSpan>,
}

pub struct Highlighter {
    pub ps: SyntaxSet,
    pub ts: ThemeSet,
}

impl Highlighter {
    pub fn new() -> Self {
        Self {
            ps: SyntaxSet::load_defaults_newlines(),
            ts: ThemeSet::load_defaults(),
        }
    }

    fn syntect_style_to_ratatui(&self, style: &syntect::highlighting::Style) -> TuiStyle {
        let fg = TuiColor::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
        let mut tui_style = TuiStyle::default().fg(fg);
        if style.font_style.contains(FontStyle::BOLD) {
            tui_style = tui_style.add_modifier(TuiModifier::BOLD);
        }
        if style.font_style.contains(FontStyle::ITALIC) {
            tui_style = tui_style.add_modifier(TuiModifier::ITALIC);
        }
        if style.font_style.contains(FontStyle::UNDERLINE) {
            tui_style = tui_style.add_modifier(TuiModifier::UNDERLINED);
        }
        tui_style
    }

    pub fn render_markdown<'a>(&self, text: &'a str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let parser = Parser::new(text);

        let mut is_bold = false;
        let mut is_italic = false;
        let mut is_link = false;
        let mut is_image = false;
        let mut code_lang: Option<String> = None;

        let mut list_stack: Vec<ListState> = Vec::new();
        let mut blockquote_depth = 0;

        let mut current_spans: Vec<StyledSpan> = Vec::new();

        let mut table_alignments = Vec::new();
        let mut table_rows: Vec<Vec<TableCell>> = Vec::new();
        let mut current_row: Vec<TableCell> = Vec::new();
        let mut in_table = false;

        for event in parser {
            match event {
                Event::Start(tag) => {
                    match tag {
                        Tag::Heading(_, _, _) => {
                            current_spans.clear();
                        }
                        Tag::Paragraph => {
                            current_spans.clear();
                        }
                        Tag::BlockQuote => {
                            blockquote_depth += 1;
                        }
                        Tag::List(first_num) => {
                            list_stack.push(ListState {
                                is_ordered: first_num.is_some(),
                                index: first_num.unwrap_or(1),
                                item_has_task_marker: false,
                                item_checked: false,
                            });
                        }
                        Tag::Item => {
                            current_spans.clear();
                            if let Some(list) = list_stack.last_mut() {
                                list.item_has_task_marker = false;
                                list.item_checked = false;
                            }
                        }
                        Tag::Link(_, _, _) => {
                            is_link = true;
                        }
                        Tag::Image(_, _, _) => {
                            is_image = true;
                        }
                        Tag::Emphasis => {
                            is_italic = true;
                        }
                        Tag::Strong => {
                            is_bold = true;
                        }
                        Tag::CodeBlock(kind) => {
                            code_lang = match kind {
                                CodeBlockKind::Fenced(lang) => Some(lang.to_string()),
                                CodeBlockKind::Indented => None,
                            };
                            current_spans.clear();
                        }
                        Tag::Table(alignments) => {
                            in_table = true;
                            table_alignments = alignments;
                            table_rows.clear();
                            current_row.clear();
                        }
                        Tag::TableRow => {
                            current_row.clear();
                        }
                        Tag::TableCell => {
                            current_spans.clear();
                        }
                        _ => {}
                    }
                }
                Event::End(tag) => {
                    match tag {
                        Tag::Heading(level, _, _) => {
                            let style = match level {
                                HeadingLevel::H1 => theme.header1,
                                HeadingLevel::H2 => theme.header2,
                                _ => theme.header3,
                            };

                            for span in current_spans.iter_mut() {
                                span.style = style.clone();
                            }

                            let level_prefix = match level {
                                HeadingLevel::H1 => "# ",
                                HeadingLevel::H2 => "## ",
                                HeadingLevel::H3 => "### ",
                                HeadingLevel::H4 => "#### ",
                                HeadingLevel::H5 => "##### ",
                                HeadingLevel::H6 => "###### ",
                            };

                            let words = tokenize_spans_into_words(&current_spans);
                            let heading_lines = wrap_words(
                                &words,
                                width.saturating_sub(4),
                                level_prefix,
                                &" ".repeat(level_prefix.len()),
                            );

                            lines.extend(heading_lines);
                            lines.push(Line::from(""));
                            current_spans.clear();
                        }
                        Tag::Paragraph => {
                            if in_table {
                                continue;
                            }
                            let words = tokenize_spans_into_words(&current_spans);
                            let bq_prefix = "│ ".repeat(blockquote_depth);
                            let max_w = width.saturating_sub(4 + bq_prefix.chars().count());

                            let para_lines = wrap_words(
                                &words,
                                max_w,
                                &bq_prefix,
                                &bq_prefix,
                            );

                            lines.extend(para_lines);
                            lines.push(Line::from(""));
                            current_spans.clear();
                        }
                        Tag::BlockQuote => {
                            blockquote_depth = blockquote_depth.saturating_sub(1);
                        }
                        Tag::List(_) => {
                            list_stack.pop();
                        }
                        Tag::Item => {
                            let mut first_prefix = String::new();
                            let mut sub_prefix = String::new();

                            let bq_prefix = "│ ".repeat(blockquote_depth);
                            first_prefix.push_str(&bq_prefix);
                            sub_prefix.push_str(&bq_prefix);

                            if list_stack.len() > 1 {
                                let indent = "  ".repeat(list_stack.len() - 1);
                                first_prefix.push_str(&indent);
                                sub_prefix.push_str(&indent);
                            }

                            let mut has_task = false;
                            let mut checked = false;

                            if let Some(list) = list_stack.last_mut() {
                                has_task = list.item_has_task_marker;
                                checked = list.item_checked;

                                if list.is_ordered {
                                    let num_str = format!("{}. ", list.index);
                                    first_prefix.push_str(&num_str);
                                    sub_prefix.push_str(&" ".repeat(num_str.len()));
                                    list.index += 1;
                                } else {
                                    first_prefix.push_str("• ");
                                    sub_prefix.push_str("  ");
                                }
                            }

                            if has_task {
                                let checkbox_str = if checked { "[✓] " } else { "[ ] " };
                                first_prefix.push_str(checkbox_str);
                                sub_prefix.push_str("    ");
                            }

                            let words = tokenize_spans_into_words(&current_spans);
                            let max_w = width.saturating_sub(4 + sub_prefix.chars().count());

                            let item_lines = wrap_words(
                                &words,
                                max_w,
                                &first_prefix,
                                &sub_prefix,
                            );

                            lines.extend(item_lines);
                            current_spans.clear();
                        }
                        Tag::Link(_, _, _) => {
                            is_link = false;
                        }
                        Tag::Image(_, _, _) => {
                            is_image = false;
                        }
                        Tag::Emphasis => {
                            is_italic = false;
                        }
                        Tag::Strong => {
                            is_bold = false;
                        }
                        Tag::CodeBlock(_) => {
                            let mut code_lines = Vec::new();
                            let bq_prefix = "│ ".repeat(blockquote_depth);
                            let max_w = width.saturating_sub(4 + bq_prefix.chars().count());

                            let mut raw_code = String::new();
                            for s in &current_spans {
                                raw_code.push_str(&s.text);
                            }

                            let syntax = match &code_lang {
                                Some(lang) => self.ps.find_syntax_by_token(lang)
                                    .unwrap_or_else(|| self.ps.find_syntax_plain_text()),
                                None => self.ps.find_syntax_plain_text(),
                            };
                            
                            let theme_syntect = &self.ts.themes["base16-ocean.dark"];
                            let mut h = HighlightLines::new(syntax, theme_syntect);

                            for line in raw_code.lines() {
                                let mut line_spans = Vec::new();
                                if !bq_prefix.is_empty() {
                                    line_spans.push(Span::raw(bq_prefix.clone()));
                                }

                                let mut highlighted_line_spans = Vec::new();
                                if let Ok(ranges) = h.highlight_line(line, &self.ps) {
                                    for (style, text) in ranges {
                                        let tui_style = self.syntect_style_to_ratatui(&style).bg(theme.code_block_bg);
                                        highlighted_line_spans.push(Span::styled(text.to_string(), tui_style));
                                    }
                                } else {
                                    highlighted_line_spans.push(Span::styled(
                                        line.to_string(), 
                                        TuiStyle::default().fg(TuiColor::Yellow).bg(theme.code_block_bg)
                                    ));
                                }

                                let current_line_len: usize = highlighted_line_spans.iter().map(|s| s.content.chars().count()).sum();
                                let pad_len = max_w.saturating_sub(current_line_len + 1);
                                
                                // Add a leading space for padding as in original
                                let mut final_spans = Vec::new();
                                if !bq_prefix.is_empty() {
                                    final_spans.push(Span::raw(bq_prefix.clone()));
                                }
                                final_spans.push(Span::styled(" ", TuiStyle::default().bg(theme.code_block_bg)));
                                final_spans.extend(highlighted_line_spans);
                                final_spans.push(Span::styled(" ".repeat(pad_len), TuiStyle::default().bg(theme.code_block_bg)));
                                
                                code_lines.push(Line::from(final_spans));
                            }

                            lines.extend(code_lines);
                            lines.push(Line::from(""));
                            code_lang = None;
                            current_spans.clear();
                        }
                        Tag::Table(_) => {
                            let num_cols = table_alignments.len();
                            let mut col_widths = vec![3; num_cols];
                            for row in &table_rows {
                                for (i, cell) in row.iter().enumerate().take(num_cols) {
                                    let cell_text_len: usize = cell.spans.iter().map(|s| s.text.chars().count()).sum();
                                    col_widths[i] = col_widths[i].max(cell_text_len);
                                }
                            }

                            let bq_prefix = "│ ".repeat(blockquote_depth);

                            for (row_idx, row) in table_rows.iter().enumerate() {
                                if row_idx == 0 {
                                    let mut top_line = bq_prefix.clone();
                                    top_line.push('┌');
                                    for (i, w) in col_widths.iter().enumerate() {
                                        top_line.push_str(&"─".repeat(*w + 2));
                                        if i < col_widths.len() - 1 {
                                            top_line.push('┬');
                                        }
                                    }
                                    top_line.push('┐');
                                    lines.push(Line::from(top_line));
                                }

                                let mut row_line_spans = Vec::new();
                                row_line_spans.push(Span::raw(bq_prefix.clone() + "│"));

                                for (i, w) in col_widths.iter().enumerate() {
                                    let cell = row.get(i);
                                    let mut cell_spans = Vec::new();
                                    let mut cell_len = 0;
                                    if let Some(c) = cell {
                                        for s in &c.spans {
                                            cell_spans.push(Span::styled(s.text.clone(), s.style.clone()));
                                            cell_len += s.text.chars().count();
                                        }
                                    }

                                    let align = table_alignments.get(i).cloned().unwrap_or(Alignment::None);
                                    let pad = w.saturating_sub(cell_len);
                                    let (pad_left, pad_right) = match align {
                                        Alignment::Left | Alignment::None => (1, pad + 1),
                                        Alignment::Right => (pad + 1, 1),
                                        Alignment::Center => {
                                            let left = pad / 2 + 1;
                                            let right = pad - pad / 2 + 1;
                                            (left, right)
                                        }
                                    };

                                    row_line_spans.push(Span::raw(" ".repeat(pad_left)));
                                    row_line_spans.extend(cell_spans);
                                    row_line_spans.push(Span::raw(" ".repeat(pad_right) + "│"));
                                }

                                lines.push(Line::from(row_line_spans));

                                if row_idx == 0 {
                                    let mut sep_line = bq_prefix.clone();
                                    sep_line.push('├');
                                    for (i, w) in col_widths.iter().enumerate() {
                                        sep_line.push_str(&"─".repeat(*w + 2));
                                        if i < col_widths.len() - 1 {
                                            sep_line.push('┼');
                                        }
                                    }
                                    sep_line.push('┤');
                                    lines.push(Line::from(sep_line));
                                }
                            }

                            if !table_rows.is_empty() {
                                let mut bot_line = bq_prefix.clone();
                                bot_line.push('└');
                                for (i, w) in col_widths.iter().enumerate() {
                                    bot_line.push_str(&"─".repeat(*w + 2));
                                    if i < col_widths.len() - 1 {
                                        bot_line.push('┴');
                                    }
                                }
                                bot_line.push('┘');
                                lines.push(Line::from(bot_line));
                            }

                            lines.push(Line::from(""));
                            in_table = false;
                        }
                        Tag::TableRow => {
                            table_rows.push(current_row.clone());
                        }
                        Tag::TableCell => {
                            current_row.push(TableCell { spans: current_spans.clone() });
                            current_spans.clear();
                        }
                        _ => {}
                    }
                }
                Event::Text(text) => {
                    let mut style = TuiStyle::default().fg(theme.fg);
                    if is_bold {
                        style = style.patch(theme.bold.clone());
                    }
                    if is_italic {
                        style = style.patch(theme.italic.clone());
                    }
                    if is_link {
                        style = style.patch(theme.link.clone());
                    }
                    if is_image {
                        style = style.patch(theme.image.clone());
                    }

                    current_spans.push(StyledSpan {
                        text: text.to_string(),
                        style,
                    });
                }
                Event::Code(text) => {
                    current_spans.push(StyledSpan {
                        text: format!("`{}`", text),
                        style: theme.code_inline.clone(),
                    });
                }
                Event::Rule => {
                    let rule_str = "—".repeat(width.saturating_sub(6));
                    lines.push(Line::from(vec![Span::styled(rule_str, theme.hr.clone())]));
                    lines.push(Line::from(""));
                }
                Event::SoftBreak => {
                    current_spans.push(StyledSpan {
                        text: " ".to_string(),
                        style: TuiStyle::default(),
                    });
                }
                Event::HardBreak => {
                    current_spans.push(StyledSpan {
                        text: "\n".to_string(),
                        style: TuiStyle::default(),
                    });
                }
                Event::TaskListMarker(checked) => {
                    if let Some(list) = list_stack.last_mut() {
                        list.item_has_task_marker = true;
                        list.item_checked = checked;
                    }
                }
                _ => {}
            }
        }

        lines
    }
}

pub fn draw_ui<'a, B: ratatui::backend::Backend>(f: &mut Frame<'a, B>, editor: &mut Editor, highlighter: &Highlighter) {
    let area = f.size();
    
    // If the terminal is too small (< 40 columns)
    if area.width < 40 {
        let msg = Paragraph::new("Terminal is too small (< 40 cols). Please resize.")
            .style(TuiStyle::default().fg(TuiColor::Red));
        f.render_widget(msg, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(1), // Tab Bar
                Constraint::Min(3),    // Main Editor / Preview Split
                Constraint::Length(1), // Status Bar
                Constraint::Length(1), // Command Line
            ]
            .as_ref(),
        )
        .split(area);

    draw_tab_bar(f, editor, chunks[0]);
    draw_main_split(f, editor, chunks[1], highlighter);
    draw_status_bar(f, editor);
    draw_command_area(f, editor, chunks[3]);

    if editor.show_help {
        draw_help_overlay(f, area);
    }
}

fn draw_tab_bar<B: ratatui::backend::Backend>(f: &mut Frame<B>, editor: &Editor, area: Rect) {
    let mut spans = Vec::new();
    for (idx, buffer) in editor.buffers.iter().enumerate() {
        let name = match &buffer.path {
            Some(path) => path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            None => "[No Name]".to_string(),
        };
        let mod_indicator = if buffer.modified { " +" } else { "" };
        let tab_text = format!(" {}{}{} ", if idx == editor.current_buffer_idx { "[" } else { " " }, name, mod_indicator);
        
        let style = if idx == editor.current_buffer_idx {
            TuiStyle::default().fg(TuiColor::Yellow).bg(TuiColor::DarkGray).add_modifier(TuiModifier::BOLD)
        } else {
            TuiStyle::default().fg(TuiColor::Gray).bg(TuiColor::Black)
        };
        spans.push(Span::styled(tab_text, style));
    }
    
    let paragraph = Paragraph::new(Line::from(spans)).style(TuiStyle::default().bg(TuiColor::Black));
    f.render_widget(paragraph, area);
}

fn draw_main_split<B: ratatui::backend::Backend>(f: &mut Frame<B>, editor: &mut Editor, area: Rect, highlighter: &Highlighter) {
    let split_chunks = if editor.preview_enabled {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100)].as_ref())
            .split(area)
    };

    draw_editor_pane(f, editor, split_chunks[0], highlighter);

    if editor.preview_enabled {
        draw_preview_pane(f, editor, split_chunks[1], highlighter);
    }
}

fn draw_editor_pane<B: ratatui::backend::Backend>(f: &mut Frame<B>, editor: &mut Editor, area: Rect, highlighter: &Highlighter) {
    let buffer = &mut editor.buffers[editor.current_buffer_idx];
    let editor_height = area.height as usize;
    let theme_colors = Theme::get_by_name(&editor.config.theme);

    // Keep cursor visible inside scroll margins
    if buffer.cursor_line < buffer.scroll_row {
        buffer.scroll_row = buffer.cursor_line;
    } else if buffer.cursor_line >= buffer.scroll_row + editor_height {
        buffer.scroll_row = buffer.cursor_line - editor_height + 1;
    }

    let max_col_idx = area.width as usize;
    if buffer.cursor_col < buffer.scroll_col {
        buffer.scroll_col = buffer.cursor_col;
    } else if buffer.cursor_col >= buffer.scroll_col + max_col_idx.saturating_sub(6) {
        buffer.scroll_col = buffer.cursor_col - max_col_idx.saturating_sub(6) + 1;
    }

    let mut lines = Vec::new();
    let syntax = highlighter.ps.find_syntax_by_extension("md")
        .unwrap_or_else(|| highlighter.ps.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, &highlighter.ts.themes["base16-ocean.dark"]);

    // Calculate all regex matches
    let matches = if editor.search_state.active {
        crate::search::find_matches(buffer, &editor.search_state.pattern)
    } else {
        Vec::new()
    };

    for row_offset in 0..editor_height {
        let line_idx = buffer.scroll_row + row_offset;
        
        if line_idx < buffer.rope.len_lines() {
            let line_raw = buffer.rope.line(line_idx).to_string();
            let mut line_clean = line_raw.clone();
            while line_clean.ends_with('\n') || line_clean.ends_with('\r') {
                line_clean.pop();
            }

            // Highlighting colors array
            let mut char_styles = vec![
                TuiStyle::default().fg(theme_colors.fg);
                line_clean.chars().count()
            ];

            if editor.config.syntax_highlight {
                if let Ok(ranges) = h.highlight_line(&line_raw, &highlighter.ps) {
                    let mut curr_char_idx = 0;
                    for (style, text) in ranges {
                        let text_count = text.chars().count();
                        let fg_color = TuiColor::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                        let mut tui_style = TuiStyle::default().fg(fg_color);
                        if style.font_style.contains(FontStyle::BOLD) {
                            tui_style = tui_style.add_modifier(TuiModifier::BOLD);
                        }
                        if style.font_style.contains(FontStyle::ITALIC) {
                            tui_style = tui_style.add_modifier(TuiModifier::ITALIC);
                        }
                        if style.font_style.contains(FontStyle::UNDERLINE) {
                            tui_style = tui_style.add_modifier(TuiModifier::UNDERLINED);
                        }

                        for _ in 0..text_count {
                            if curr_char_idx < char_styles.len() {
                                char_styles[curr_char_idx] = tui_style;
                                curr_char_idx += 1;
                            }
                        }
                    }
                }
            }

            // Selection highlights overlay
            if editor.mode == Mode::Visual {
                if let Some(anchor) = editor.visual_anchor {
                    let cursor = (buffer.cursor_line, buffer.cursor_col);
                    for col_idx in 0..line_clean.chars().count() {
                        if is_selected(anchor, cursor, line_idx, col_idx) {
                            char_styles[col_idx] = char_styles[col_idx]
                                .bg(TuiColor::Rgb(60, 70, 90))
                                .fg(TuiColor::White);
                        }
                    }
                }
            }

            // Search matches overlay
            for &(m_line, m_start, m_end) in &matches {
                if m_line == line_idx {
                    for col_idx in m_start..m_end {
                        if col_idx < char_styles.len() {
                            char_styles[col_idx] = char_styles[col_idx]
                                .bg(TuiColor::Yellow)
                                .fg(TuiColor::Black);
                        }
                    }
                }
            }

            let mut line_spans = Vec::new();
            
            // Render line number
            if editor.config.show_line_numbers {
                let num_str = format!("{:4} ", line_idx + 1);
                line_spans.push(Span::styled(num_str, TuiStyle::default().fg(theme_colors.line_number_fg)));
            }

            // Render current character slices taking scroll column offset into account
            let chars: Vec<char> = line_clean.chars().collect();
            for col_idx in buffer.scroll_col..(buffer.scroll_col + max_col_idx.saturating_sub(6)).min(chars.len()) {
                line_spans.push(Span::styled(chars[col_idx].to_string(), char_styles[col_idx]));
            }

            lines.push(Line::from(line_spans));
        } else {
            // Tilde marker for empty line
            let mut line_spans = Vec::new();
            if editor.config.show_line_numbers {
                line_spans.push(Span::raw("     "));
            }
            line_spans.push(Span::styled("~", TuiStyle::default().fg(TuiColor::DarkGray)));
            lines.push(Line::from(line_spans));
        }
    }

    let mut block = Block::default().style(TuiStyle::default().bg(theme_colors.bg));
    if editor.preview_enabled {
        block = block.borders(Borders::RIGHT).border_style(TuiStyle::default().fg(TuiColor::DarkGray));
    }

    f.render_widget(Paragraph::new(lines).block(block), area);

    // Set cursor on terminal
    let mut cursor_col = buffer.cursor_col.saturating_sub(buffer.scroll_col);
    if editor.config.show_line_numbers {
        cursor_col += 5;
    }
    let cursor_row = buffer.cursor_line.saturating_sub(buffer.scroll_row);

    if cursor_row < area.height as usize && cursor_col < area.width as usize {
        f.set_cursor(
            (area.x as usize + cursor_col) as u16,
            (area.y as usize + cursor_row) as u16,
        );
    }
}

fn draw_preview_pane<B: ratatui::backend::Backend>(f: &mut Frame<B>, editor: &Editor, area: Rect, highlighter: &Highlighter) {
    let buffer = &editor.buffers[editor.current_buffer_idx];
    let theme_colors = Theme::get_by_name(&editor.config.theme);
    
    let raw_text = buffer.rope.to_string();
    let width = area.width as usize;
    let preview_lines = highlighter.render_markdown(&raw_text, width, &theme_colors);

    // Dynamic split scrolling logic
    // Ensure scroll depth aligns proportionally to editor cursor position
    let editor_total_lines = buffer.rope.len_lines();
    let scroll_y = if editor_total_lines > 0 {
        let ratio = buffer.cursor_line as f64 / editor_total_lines as f64;
        (preview_lines.len() as f64 * ratio).floor() as usize
    } else {
        0
    };

    let visible_lines: Vec<Line<'static>> = preview_lines
        .into_iter()
        .skip(scroll_y)
        .take(area.height as usize)
        .collect();

    f.render_widget(
        Paragraph::new(visible_lines).style(TuiStyle::default().bg(theme_colors.bg)),
        area,
    );
}

fn draw_status_bar<B: ratatui::backend::Backend>(f: &mut Frame<B>, editor: &Editor) {
    let area = f.size();
    let row = area.height - 2;
    let bar_area = Rect::new(0, row, area.width, 1);
    let theme_colors = Theme::get_by_name(&editor.config.theme);

    let mode_str = match editor.mode {
        Mode::Normal => " -- NORMAL -- ",
        Mode::Insert => " -- INSERT -- ",
        Mode::Visual => " -- VISUAL -- ",
        Mode::Command => " -- COMMAND -- ",
    };

    let mode_style = match editor.mode {
        Mode::Normal => TuiStyle::default().fg(TuiColor::Black).bg(TuiColor::Blue).add_modifier(TuiModifier::BOLD),
        Mode::Insert => TuiStyle::default().fg(TuiColor::Black).bg(TuiColor::Green).add_modifier(TuiModifier::BOLD),
        Mode::Visual => TuiStyle::default().fg(TuiColor::Black).bg(TuiColor::Magenta).add_modifier(TuiModifier::BOLD),
        Mode::Command => TuiStyle::default().fg(TuiColor::Black).bg(TuiColor::Yellow).add_modifier(TuiModifier::BOLD),
    };

    let buffer = &editor.buffers[editor.current_buffer_idx];
    let filename = match &buffer.path {
        Some(path) => path.to_string_lossy().into_owned(),
        None => "[No Name]".to_string(),
    };
    let modified_str = if buffer.modified { " *" } else { "" };
    let preview_str = if editor.preview_enabled { " [PREVIEW]" } else { "" };

    let cursor_str = format!(" L: {} C: {} ", buffer.cursor_line + 1, buffer.cursor_col + 1);

    let status_style = TuiStyle::default().fg(theme_colors.status_fg).bg(theme_colors.status_bg);

    let left_spans = vec![
        Span::styled(mode_str, mode_style),
        Span::styled(format!("  {}{}{} ", filename, modified_str, preview_str), status_style),
    ];
    let right_span = Span::styled(cursor_str, status_style);

    // Pad space to align cursor position block on right side of screen
    let left_width: usize = left_spans.iter().map(|s| s.content.chars().count()).sum();
    let right_width = right_span.content.chars().count();
    let pad_len = (area.width as usize).saturating_sub(left_width + right_width);

    let mut bar_spans = left_spans;
    bar_spans.push(Span::styled(" ".repeat(pad_len), status_style));
    bar_spans.push(right_span);

    f.render_widget(Paragraph::new(Line::from(bar_spans)).style(TuiStyle::default().bg(theme_colors.status_bg)), bar_area);
}

fn draw_command_area<B: ratatui::backend::Backend>(f: &mut Frame<B>, editor: &Editor, area: Rect) {
    let line = if editor.mode == Mode::Command {
        let prefix = match editor.command_type {
            Some(crate::editor::CommandType::Colon) => ":",
            Some(crate::editor::CommandType::Slash) => "/",
            Some(crate::editor::CommandType::Question) => "?",
            None => "",
        };
        Line::from(format!("{}{}", prefix, editor.command_buffer))
    } else if let Some((msg, is_error)) = &editor.message {
        let style = if *is_error {
            TuiStyle::default().fg(TuiColor::Red).add_modifier(TuiModifier::BOLD)
        } else {
            TuiStyle::default().fg(TuiColor::Green)
        };
        Line::from(Span::styled(msg.clone(), style))
    } else {
        Line::from("")
    };

    f.render_widget(Paragraph::new(line), area);
}

fn draw_help_overlay<B: ratatui::backend::Backend>(f: &mut Frame<B>, area: Rect) {
    let help_text = vec![
        Line::from(vec![Span::styled(" mdterm Keybindings HELP ", TuiStyle::default().fg(TuiColor::Yellow).add_modifier(TuiModifier::BOLD))]),
        Line::from(""),
        Line::from("Normal Mode (Esc to return):"),
        Line::from("  h/j/k/l or Arrows - Move cursor"),
        Line::from("  i/a/I/A           - Enter Insert Mode"),
        Line::from("  o/O               - Open new line below/above"),
        Line::from("  x/dd              - Delete character/current line"),
        Line::from("  yy/p/P            - Yank line, paste after/before"),
        Line::from("  u/Ctrl+R          - Undo / Redo"),
        Line::from("  /pattern          - Search forward"),
        Line::from("  ?pattern          - Search backward"),
        Line::from("  n/N               - Next / Previous search match"),
        Line::from("  v                 - Visual Mode"),
        Line::from("  Ctrl+S            - Save file"),
        Line::from("  Ctrl+P            - Toggle Live Preview split"),
        Line::from("  Ctrl+Tab          - Next buffer"),
        Line::from("  Ctrl+Shift+Tab    - Previous buffer"),
        Line::from(""),
        Line::from("Command Line Mode (:):"),
        Line::from("  :w                - Save"),
        Line::from("  :q                - Quit (warns if unsaved)"),
        Line::from("  :wq or :x         - Save and quit"),
        Line::from("  :q!               - Force quit without saving"),
        Line::from("  :e <filename>     - Open file in current buffer"),
        Line::from("  :new              - Open empty buffer"),
        Line::from("  :saveas <file>    - Save as new file path"),
        Line::from("  :bn / :bp         - Next / Previous buffer"),
        Line::from("  :set [no]number   - Toggle line numbers"),
        Line::from("  :colorscheme <t>  - Change theme (dark/light)"),
        Line::from("  :s/old/new/g      - Replace regex on current line"),
        Line::from("  :%s/old/new/g     - Replace regex globally in file"),
        Line::from(""),
        Line::from(vec![Span::styled(" Press Esc or any key to close help overlay ", TuiStyle::default().fg(TuiColor::Gray).add_modifier(TuiModifier::ITALIC))]),
    ];

    let block = Block::default()
        .title(" Help Overlay ")
        .borders(Borders::ALL)
        .border_style(TuiStyle::default().fg(TuiColor::Yellow));
    let paragraph = Paragraph::new(help_text).block(block).style(TuiStyle::default().bg(TuiColor::Rgb(30, 30, 30)));

    let w = 60.min(area.width);
    let h = 25.min(area.height);
    let x = (area.width - w) / 2;
    let y = (area.height - h) / 2;
    let rect = Rect::new(x, y, w, h);

    f.render_widget(Clear, rect);
    f.render_widget(paragraph, rect);
}

// ... (rest of the file)

fn is_selected(anchor: (usize, usize), cursor: (usize, usize), y: usize, x: usize) -> bool {
    let (min_y, max_y) = if anchor.0 < cursor.0 {
        (anchor, cursor)
    } else if anchor.0 > cursor.0 {
        (cursor, anchor)
    } else {
        let (min_x, max_x) = if anchor.1 < cursor.1 {
            (anchor.1, cursor.1)
        } else {
            (cursor.1, anchor.1)
        };
        return y == anchor.0 && x >= min_x && x <= max_x;
    };

    if y > min_y.0 && y < max_y.0 {
        true
    } else if y == min_y.0 {
        x >= min_y.1
    } else if y == max_y.0 {
        x <= max_y.1
    } else {
        false
    }
}

fn tokenize_spans_into_words(spans: &[StyledSpan]) -> Vec<StyledWord> {
    let mut words = Vec::new();
    let mut current_word = String::new();

    for span in spans {
        for c in span.text.chars() {
            if c.is_whitespace() {
                if !current_word.is_empty() {
                    words.push(StyledWord {
                        text: current_word.clone(),
                        style: span.style.clone(),
                    });
                    current_word.clear();
                }
                words.push(StyledWord {
                    text: c.to_string(),
                    style: span.style.clone(),
                });
            } else {
                current_word.push(c);
            }
        }
        if !current_word.is_empty() {
            words.push(StyledWord {
                text: current_word.clone(),
                style: span.style.clone(),
            });
            current_word.clear();
        }
    }
    words
}

fn wrap_words(
    words: &[StyledWord],
    max_width: usize,
    first_line_prefix: &str,
    subsequent_line_prefix: &str,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if words.is_empty() {
        if !first_line_prefix.is_empty() {
            lines.push(Line::from(first_line_prefix.to_string()));
        }
        return lines;
    }

    let mut current_line_spans: Vec<Span<'static>> = Vec::new();
    let mut current_width = first_line_prefix.chars().count();
    let mut is_first_line = true;

    let prefix = first_line_prefix;
    if !prefix.is_empty() {
        current_line_spans.push(Span::raw(prefix.to_string()));
    }

    for word in words {
        let word_len = word.text.chars().count();

        if word.text == "\n" {
            lines.push(Line::from(current_line_spans.clone()));
            current_line_spans.clear();
            is_first_line = false;

            let prefix = subsequent_line_prefix;
            if !prefix.is_empty() {
                current_line_spans.push(Span::raw(prefix.to_string()));
            }
            current_width = prefix.chars().count();
            continue;
        }

        let is_space = word.text.chars().all(|c| c.is_whitespace());
        if is_space && current_width == (if is_first_line { first_line_prefix } else { subsequent_line_prefix }).chars().count() {
            continue;
        }

        if current_width + word_len > max_width && current_width > (if is_first_line { first_line_prefix } else { subsequent_line_prefix }).chars().count() {
            lines.push(Line::from(current_line_spans.clone()));
            current_line_spans.clear();
            is_first_line = false;

            let prefix = subsequent_line_prefix;
            if !prefix.is_empty() {
                current_line_spans.push(Span::raw(prefix.to_string()));
            }
            current_width = prefix.chars().count();
        }

        current_line_spans.push(Span::styled(word.text.clone(), word.style.clone()));
        current_width += word_len;
    }

    if current_width > (if is_first_line { first_line_prefix } else { subsequent_line_prefix }).chars().count() {
        lines.push(Line::from(current_line_spans));
    }

    lines
}
