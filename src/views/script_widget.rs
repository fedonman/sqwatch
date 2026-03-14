use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};
use regex::Regex;
use std::process::Command;
use std::sync::LazyLock;

use crate::backend::commands::scontrol_show_job;

static ANSI_ESCAPE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1B\[([0-9;]*)m").unwrap());

pub struct ScriptWidget {
    pub job_id: Option<String>,
    pub job_name: Option<String>,
    pub body: String,
    pub scroll_pos: usize,
    pub path: Option<String>,
    pub has_bat: bool,
}

impl ScriptWidget {
    pub fn new() -> Self {
        Self {
            job_id: None,
            job_name: None,
            body: String::new(),
            scroll_pos: 0,
            path: None,
            has_bat: detect_bat(),
        }
    }

    pub fn switch_job(&mut self, id: String, label: String) {
        self.job_id = Some(id);
        self.job_name = Some(label);
        self.path = None;
        self.scroll_pos = 0;
        self.load_content();
    }

    pub fn scroll_up(&mut self) {
        self.scroll_pos = self.scroll_pos.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        let max = self.body.lines().count() * 2;
        if self.scroll_pos < max {
            self.scroll_pos += 1;
        }
    }

    pub fn ensure_job(&mut self, job_id: &str, job_name: &str) {
        if self.job_id.as_deref() == Some(job_id) {
            return;
        }
        self.switch_job(job_id.to_string(), job_name.to_string());
    }

    pub fn clear_job(&mut self) {
        self.job_id = None;
        self.job_name = None;
        self.body.clear();
        self.scroll_pos = 0;
    }

    pub fn render_inline(&self, frame: &mut Frame, area: Rect, focused: bool) {
        let border_color = if focused {
            Color::Magenta
        } else {
            Color::Rgb(80, 80, 110)
        };

        let title = match (&self.job_id, &self.job_name) {
            (Some(id), Some(name)) => format!(" Script: {}/{} ", name, id),
            _ => " Script ".to_string(),
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color));

        if self.job_id.is_none() {
            let placeholder = Paragraph::new("Select a job to view its script")
                .style(Style::default().fg(Color::DarkGray))
                .block(block);
            frame.render_widget(placeholder, area);
            return;
        }

        let display = self.build_display_text();
        let widget = Paragraph::new(display)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll_pos as u16, 0));

        frame.render_widget(widget, area);
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (_, KeyCode::Up) => self.scroll_up(),
            (_, KeyCode::Down) => self.scroll_down(),
            _ => {}
        }
    }

    fn build_display_text(&self) -> Text<'_> {
        if self.has_bat {
            return Text::from(ansi_to_spans(&self.body));
        }

        let lines: Vec<&str> = self.body.lines().collect();
        let num_width = lines.len().to_string().len() + 1;

        let numbered: Vec<Line> = lines
            .iter()
            .enumerate()
            .map(|(i, text)| {
                Line::from(vec![
                    Span::styled(
                        format!("{:>w$} ", i + 1, w = num_width - 1),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(*text),
                ])
            })
            .collect();

        Text::from(numbered)
    }

    fn load_content(&mut self) {
        let Some(id) = &self.job_id else {
            self.body.clear();
            return;
        };

        let Some(detail) = scontrol_show_job(id) else {
            self.body = "Failed to retrieve job information".into();
            return;
        };

        let Some(script_path) = detail.command else {
            self.body = "No script found for this job. Maybe it's wrapped".into();
            return;
        };

        self.path = Some(script_path.clone());

        if self.has_bat
            && let Some(highlighted) = run_bat(&script_path)
        {
            self.body = highlighted;
            return;
        }

        self.has_bat = false;
        match std::fs::read_to_string(&script_path) {
            Ok(content) => self.body = content,
            Err(_) => {
                self.body = format!("Failed to read script from path: {}", script_path);
            }
        }
    }
}

fn run_bat(path: &str) -> Option<String> {
    let result = Command::new("bat")
        .arg("--style=numbers,grid")
        .arg("--color=always")
        .arg("--theme")
        .arg("Solarized (light)")
        .arg("--terminal-width=100")
        .arg(path)
        .output();

    match result {
        Ok(out) if out.status.success() => Some(String::from_utf8_lossy(&out.stdout).into_owned()),
        _ => None,
    }
}

fn ansi_to_spans(text: &str) -> Vec<Line<'_>> {
    let mut output = Vec::new();
    let mut style = Style::default();

    for line in text.lines() {
        let mut spans = Vec::new();
        let mut cursor = 0;

        for cap in ANSI_ESCAPE_RE.captures_iter(line) {
            let whole = cap.get(0).unwrap();
            let codes = cap.get(1).unwrap();

            if whole.start() > cursor {
                spans.push(Span::styled(&line[cursor..whole.start()], style));
            }

            style = apply_ansi_codes(codes.as_str(), style);
            cursor = whole.end();
        }

        if cursor < line.len() {
            let tail = &line[cursor..];
            if !tail.is_empty() {
                spans.push(Span::styled(tail, style));
            }
        }

        style = Style::default();

        if spans.is_empty() {
            output.push(Line::default());
        } else {
            output.push(Line::from(spans));
        }
    }

    output
}

fn apply_ansi_codes(codes_str: &str, mut style: Style) -> Style {
    let segments: Vec<&str> = codes_str.split(';').collect();
    let mut it = segments.iter();

    while let Some(&seg) = it.next() {
        match seg {
            "0" => style = Style::default(),
            "1" => style = style.add_modifier(Modifier::BOLD),
            "3" => style = style.add_modifier(Modifier::ITALIC),
            "4" => style = style.add_modifier(Modifier::UNDERLINED),
            "38" => {
                if let Some(&"5") = it.next()
                    && let Some(&idx_str) = it.next()
                    && let Ok(idx) = idx_str.parse::<u8>()
                {
                    style = style.fg(Color::Indexed(idx));
                }
            }
            "48" => {
                if let Some(&"5") = it.next()
                    && let Some(&idx_str) = it.next()
                    && let Ok(idx) = idx_str.parse::<u8>()
                {
                    style = style.bg(Color::Indexed(idx));
                }
            }
            s if s.len() <= 3 && s.starts_with("3") => {
                if let Ok(n) = s[1..].parse::<u8>() {
                    style = style.fg(Color::Indexed(n));
                }
            }
            s if s.len() <= 3 && s.starts_with("9") => {
                if let Ok(n) = s[1..].parse::<u8>() {
                    style = style.fg(Color::Indexed(n + 8));
                }
            }
            s if s.len() <= 3 && s.starts_with("4") && s != "48" => {
                if let Ok(n) = s[1..].parse::<u8>() {
                    style = style.bg(Color::Indexed(n));
                }
            }
            s if s.len() <= 4 && s.starts_with("10") => {
                if let Ok(n) = s[2..].parse::<u8>() {
                    style = style.bg(Color::Indexed(n + 8));
                }
            }
            _ => {}
        }
    }

    style
}

fn detect_bat() -> bool {
    Command::new("which")
        .arg("bat")
        .output()
        .map(|o| o.status.success())
        .unwrap_or_else(|_| {
            Command::new("where")
                .arg("bat")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        })
}
