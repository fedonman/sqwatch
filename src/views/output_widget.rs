use crossbeam::channel::{Receiver, unbounded};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};
use std::{path::PathBuf, time::Duration};

use crate::backend::commands::scontrol_show_job;
use crate::core::live_file::{LiveFileMonitor, MonitorError};

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
    monitor: Option<LiveFileMonitor>,
    data_rx: Option<Receiver<Result<String, MonitorError>>>,
    poll_rate: Duration,
    fstate: FileState,
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
            monitor: None,
            data_rx: None,
            poll_rate: Duration::from_secs(1),
            fstate: FileState::Missing,
        }
    }

    pub fn switch_job(&mut self, job_id: String) {
        self.job_id = Some(job_id);
        self.stdout_file = None;
        self.stderr_file = None;
        self.scroll_pos = 0;
        self.fstate = FileState::Missing;

        self.resolve_log_paths();

        if self.monitor.is_none() {
            let (tx, rx) = unbounded();
            self.monitor = Some(LiveFileMonitor::new(tx, self.poll_rate));
            self.data_rx = Some(rx);
        }

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
        let n = self.content.lines().count();
        if self.scroll_pos < n.saturating_sub(1) {
            self.scroll_pos += 1;
        }
    }

    pub fn page_up(&mut self) {
        self.scroll_pos = self.scroll_pos.saturating_sub(10);
    }

    pub fn page_down(&mut self) {
        let n = self.content.lines().count();
        self.scroll_pos = (self.scroll_pos + 10).min(n.saturating_sub(1));
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
        if let Some(m) = &mut self.monitor {
            m.set_file_path(None);
        }
    }

    pub fn render_inline(&self, frame: &mut Frame, area: Rect, focused: bool) {
        let border_color = if focused {
            Color::Magenta
        } else {
            Color::Rgb(50, 50, 70)
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

        let display_text = match (self.fstate, self.content.is_empty()) {
            (FileState::Missing, _) => format!(
                "No {} log file found for job {}",
                self.stream.label(),
                self.job_id.as_deref().unwrap_or("unknown")
            ),
            _ => self.content.clone(),
        };

        let widget = Paragraph::new(display_text)
            .style(Style::default().fg(Color::Rgb(200, 200, 210)))
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll_pos as u16, 0));

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

    fn resolve_log_paths(&mut self) {
        let Some(id) = &self.job_id else {
            self.fstate = FileState::Missing;
            return;
        };

        let Some(detail) = scontrol_show_job(id) else {
            self.fstate = FileState::Failed;
            return;
        };

        self.stdout_file = detail.stdout_file;
        self.stderr_file = detail.stderr_file;

        let has_current = match self.stream {
            StreamKind::Stdout => self.stdout_file.as_ref().is_some_and(|p| !p.is_empty()),
            StreamKind::Stderr => self.stderr_file.as_ref().is_some_and(|p| !p.is_empty()),
        };

        self.fstate = if has_current {
            FileState::Pending
        } else {
            FileState::Missing
        };
    }
}
