use crossbeam::channel::{Receiver, unbounded};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};
use std::{path::PathBuf, time::Duration};

use crate::backend::commands::JobDetail;
use crate::core::live_file::{LiveFileMonitor, MonitorError};

const POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    Stdout,
    Stderr,
}

impl StreamKind {
    fn label(&self) -> &'static str {
        match self {
            StreamKind::Stdout => "stdout",
            StreamKind::Stderr => "stderr",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileState {
    Loading,
    Missing,
    Pending,
    Failed,
}

pub struct OutputWidget {
    pub job_id: Option<String>,
    pub stream: StreamKind,
    pub content: String,
    pub scroll_pos: usize,
    pub stdout_file: Option<String>,
    pub stderr_file: Option<String>,
    max_scroll: usize,
    monitor: Option<LiveFileMonitor>,
    data_rx: Option<Receiver<Result<String, MonitorError>>>,
    fstate: FileState,
    detail_applied: bool,
}

impl OutputWidget {
    pub fn new_for(stream: StreamKind) -> Self {
        Self {
            job_id: None,
            stream,
            content: String::new(),
            scroll_pos: 0,
            stdout_file: None,
            stderr_file: None,
            max_scroll: 0,
            monitor: None,
            data_rx: None,
            fstate: FileState::Missing,
            detail_applied: false,
        }
    }

    pub fn switch_job(&mut self, job_id: String) {
        self.job_id = Some(job_id);
        self.stdout_file = None;
        self.stderr_file = None;
        self.content.clear();
        self.scroll_pos = 0;
        self.fstate = FileState::Loading;
        self.detail_applied = false;

        if self.monitor.is_none() {
            let (tx, rx) = unbounded();
            self.monitor = Some(LiveFileMonitor::new(tx, POLL_INTERVAL));
            self.data_rx = Some(rx);
        }

        // Clear the current file watch while we wait for job detail
        if let Some(m) = &mut self.monitor {
            m.set_file_path(None);
        }
    }

    /// Apply resolved job detail from the background resolver.
    /// Idempotent — returns immediately if detail was already applied.
    pub fn set_detail(&mut self, detail: &JobDetail) {
        if self.detail_applied {
            return;
        }
        self.detail_applied = true;

        self.stdout_file = detail.stdout_file.clone();
        self.stderr_file = detail.stderr_file.clone();

        let has_current = match self.stream {
            StreamKind::Stdout => self.stdout_file.as_ref().is_some_and(|p| !p.is_empty()),
            StreamKind::Stderr => self.stderr_file.as_ref().is_some_and(|p| !p.is_empty()),
        };

        self.fstate = if has_current {
            FileState::Pending
        } else {
            FileState::Missing
        };

        self.refresh_watched_file();
    }

    fn refresh_watched_file(&mut self) {
        let Some(mon) = &mut self.monitor else { return };

        let target = match self.stream {
            StreamKind::Stdout => self.stdout_file.clone(),
            StreamKind::Stderr => self.stderr_file.clone(),
        };

        match target {
            Some(p) if !p.is_empty() => {
                mon.set_file_path(Some(PathBuf::from(&p)));
                self.fstate = FileState::Pending;
            }
            _ => {
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
                Ok(text) => self.content = text,
                Err(e) => {
                    self.content = format!("Error watching file: {}", e);
                    self.fstate = FileState::Failed;
                }
            }
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

    pub fn ensure_job(&mut self, job_id: &str) {
        if self.job_id.as_deref() == Some(job_id) {
            return;
        }
        self.switch_job(job_id.to_string());
    }

    pub fn clear_job(&mut self) {
        self.job_id = None;
        self.content.clear();
        self.scroll_pos = 0;
        self.fstate = FileState::Missing;
        self.detail_applied = false;
        if let Some(m) = &mut self.monitor {
            m.set_file_path(None);
        }
    }

    pub fn render_inline(&mut self, frame: &mut Frame, area: Rect, focused: bool) {
        let border_color = if focused {
            Color::Magenta
        } else {
            Color::Rgb(80, 80, 110)
        };

        let title = match &self.job_id {
            Some(id) => format!(" {} [{}] ", self.stream.label(), id),
            None => format!(" {} ", self.stream.label()),
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color));

        if self.job_id.is_none() {
            let placeholder = Paragraph::new("Select a job to view output logs")
                .style(Style::default().fg(Color::DarkGray))
                .block(block);
            frame.render_widget(placeholder, area);
            return;
        }

        let display_text = match self.fstate {
            FileState::Loading => format!(
                "Loading {} details for job {}...",
                self.stream.label(),
                self.job_id.as_deref().unwrap_or("unknown")
            ),
            FileState::Missing => format!(
                "No {} log file found for job {}",
                self.stream.label(),
                self.job_id.as_deref().unwrap_or("unknown")
            ),
            _ if self.content.is_empty() => format!(
                "Waiting for {} content...",
                self.stream.label()
            ),
            _ => self.content.clone(),
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
