use crossbeam::channel::{Receiver, unbounded};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};
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
    monitor: Option<LiveFileMonitor>,
    data_rx: Option<Receiver<Result<String, MonitorError>>>,
    fstate: FileState,
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
            monitor: None,
            data_rx: None,
            fstate: FileState::Missing,
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
                Ok(text) => self.content = text,
                Err(e) => {
                    self.content = format!("Error watching file: {}", e);
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

    pub fn render_inline(&self, frame: &mut Frame, area: Rect, focused: bool) {
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
            FileState::Missing => format!(
                "File '{}' not found in job work directory",
                self.filename
            ),
            _ if self.content.is_empty() => format!("Waiting for content from '{}'...", self.filename),
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
}
