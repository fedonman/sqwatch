use crossbeam::channel::{Receiver, unbounded};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};
use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::core::live_file::{LiveFileMonitor, MonitorError};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const CUSTOM_ACCENT: Color = Color::Rgb(180, 130, 255);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileState {
    Missing,
    Pending,
    Failed,
}

pub struct CustomOutputWidget {
    pub def_index: usize,
    pub title: String,
    pub filename: String,
    pub job_id: Option<String>,
    pub work_dir: Option<String>,
    pub content: String,
    pub scroll_pos: usize,
    max_scroll: usize,
    monitor: Option<LiveFileMonitor>,
    data_rx: Option<Receiver<Result<String, MonitorError>>>,
    fstate: FileState,
    display_content: String,
}

impl CustomOutputWidget {
    pub fn new(def_index: usize, title: String, filename: String) -> Self {
        Self {
            def_index,
            title,
            filename,
            job_id: None,
            work_dir: None,
            content: String::new(),
            scroll_pos: 0,
            max_scroll: 0,
            monitor: None,
            data_rx: None,
            fstate: FileState::Missing,
            display_content: String::new(),
        }
    }

    pub fn switch_job(&mut self, job_id: String, work_dir: Option<&str>) {
        self.job_id = Some(job_id);
        self.work_dir = work_dir.map(String::from);
        self.content.clear();
        self.scroll_pos = 0;
        self.fstate = FileState::Missing;

        if self.monitor.is_none() {
            let (tx, rx) = unbounded();
            self.monitor = Some(LiveFileMonitor::new(tx, POLL_INTERVAL));
            self.data_rx = Some(rx);
        }

        self.refresh_watched_file();
    }

    fn resolve_path(&self) -> Option<PathBuf> {
        let work_dir = self.work_dir.as_ref()?;
        Some(Path::new(work_dir).join(&self.filename))
    }

    fn refresh_watched_file(&mut self) {
        let path = self.resolve_path();
        let Some(mon) = &mut self.monitor else { return };

        match path {
            Some(p) => {
                mon.set_file_path(Some(p));
                self.fstate = FileState::Pending;
            }
            None => {
                mon.set_file_path(None);
                self.fstate = FileState::Missing;
                self.content.clear();
            }
        }
        self.poll_updates();
    }

    pub fn poll_updates(&mut self) {
        let Some(rx) = &self.data_rx else { return };

        while let Ok(result) = rx.try_recv() {
            match result {
                Ok(text) => {
                    self.content = text;
                    self.display_content = format_content(&self.content, &self.filename);
                }
                Err(e) => {
                    self.content = format!("Error watching file: {}", e);
                    self.display_content = self.content.clone();
                    self.fstate = FileState::Failed;
                }
            }
        }
    }

    pub fn ensure_job(&mut self, job_id: &str, work_dir: Option<&str>) {
        if self.job_id.as_deref() == Some(job_id) {
            // Update work_dir if it became available after initial setup
            // (e.g., %Z wasn't in the format string when the widget was first synced)
            if work_dir.is_some() && self.work_dir.as_deref() != work_dir {
                self.work_dir = work_dir.map(String::from);
                self.refresh_watched_file();
            }
            return;
        }
        self.switch_job(job_id.to_string(), work_dir);
    }

    pub fn clear_job(&mut self) {
        self.job_id = None;
        self.work_dir = None;
        self.content.clear();
        self.scroll_pos = 0;
        self.fstate = FileState::Missing;
        if let Some(m) = &mut self.monitor {
            m.set_file_path(None);
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll_pos = self.scroll_pos.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        if self.scroll_pos < self.max_scroll {
            self.scroll_pos += 1;
        }
    }

    pub fn page_up(&mut self) {
        self.scroll_pos = self.scroll_pos.saturating_sub(10);
    }

    pub fn page_down(&mut self) {
        self.scroll_pos = (self.scroll_pos + 10).min(self.max_scroll);
    }

    pub fn render_inline(&mut self, frame: &mut Frame, area: Rect, focused: bool) {
        let border_color = if focused {
            CUSTOM_ACCENT
        } else {
            Color::Rgb(80, 80, 110)
        };

        let title = match &self.job_id {
            Some(id) => format!(" {} [{}] ", self.title, id),
            None => format!(" {} ", self.title),
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color));

        if self.job_id.is_none() {
            let placeholder = Paragraph::new("Select a job to view file contents")
                .style(Style::default().fg(Color::DarkGray))
                .block(block);
            frame.render_widget(placeholder, area);
            return;
        }

        let display_text = match self.fstate {
            FileState::Missing => {
                format!("File '{}' not found in job work directory", self.filename)
            }
            _ if self.content.is_empty() => {
                format!("Waiting for content from '{}'...", self.filename)
            }
            _ => self.display_content.clone(),
        };

        let widget = Paragraph::new(display_text)
            .style(Style::default().fg(Color::Rgb(200, 200, 210)))
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll_pos as u16, 0));

        let inner_width = area.width.saturating_sub(2);
        let inner_height = area.height.saturating_sub(2) as usize;
        let total_lines = widget.line_count(inner_width);
        self.max_scroll = total_lines.saturating_sub(inner_height);

        frame.render_widget(widget, area);
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (_, KeyCode::Up) => self.scroll_up(),
            (_, KeyCode::Down) => self.scroll_down(),
            (_, KeyCode::PageUp) | (KeyModifiers::CONTROL, KeyCode::Char('u')) => self.page_up(),
            (_, KeyCode::PageDown) | (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                self.page_down()
            }
            _ => {}
        }
    }
}

/// Formats file content for display based on file extension.
///
/// - JSON files are pretty-printed, with string fields that contain embedded
///   YAML (or JSON) expanded into structured values.
/// - YAML files are parsed and re-serialised cleanly.
/// - Unknown extensions try JSON then YAML, falling back to raw content.
fn format_content(raw: &str, filename: &str) -> String {
    if raw.is_empty() {
        return raw.to_string();
    }

    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "json" => try_format_json(raw).unwrap_or_else(|| raw.to_string()),
        "yaml" | "yml" => try_format_yaml(raw).unwrap_or_else(|| raw.to_string()),
        _ => try_format_json(raw)
            .or_else(|| try_format_yaml(raw))
            .unwrap_or_else(|| raw.to_string()),
    }
}

/// Parses JSON, expands embedded structured strings, and pretty-prints.
fn try_format_json(raw: &str) -> Option<String> {
    let mut value: JsonValue = serde_json::from_str(raw).ok()?;
    expand_embedded_strings(&mut value);
    serde_json::to_string_pretty(&value).ok()
}

/// Parses YAML and re-serialises it cleanly.
fn try_format_yaml(raw: &str) -> Option<String> {
    let value: serde_norway::Value = serde_norway::from_str(raw).ok()?;
    // Only format if the top-level value is actually structured data.
    if value.is_mapping() || value.is_sequence() {
        serde_norway::to_string(&value).ok()
    } else {
        None
    }
}

/// Walks a JSON value tree and, for every string that contains a newline,
/// tries to parse it as YAML (which is a superset of JSON). If parsing
/// produces a structured value (mapping/sequence), the string is replaced
/// with that structured value so the final pretty-print shows it expanded.
fn expand_embedded_strings(value: &mut JsonValue) {
    match value {
        JsonValue::String(s) => {
            if !s.contains('\n') {
                return;
            }
            if let Ok(parsed) = serde_norway::from_str::<serde_norway::Value>(s)
                && (parsed.is_mapping() || parsed.is_sequence())
                && let Ok(json_val) = serde_json::to_value(&parsed)
            {
                *value = json_val;
            }
        }
        JsonValue::Array(arr) => {
            for item in arr {
                expand_embedded_strings(item);
            }
        }
        JsonValue::Object(map) => {
            for v in map.values_mut() {
                expand_embedded_strings(v);
            }
        }
        _ => {}
    }
}
