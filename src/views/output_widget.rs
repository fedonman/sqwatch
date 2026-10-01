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
use crate::core::live_file::{LiveFileMonitor, LogChunk, MonitorError};
use crate::views::theme::{ACCENT_STDERR, ACCENT_STDOUT, DIM_BORDER};
use crate::views::wrap_index::WrapIndex;

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
    wrap: WrapIndex,
    monitor: Option<LiveFileMonitor>,
    data_rx: Option<Receiver<Result<LogChunk, MonitorError>>>,
    fstate: FileState,
    detail_applied: bool,
    /// When true, the view stays pinned to the tail as new content arrives.
    follow: bool,
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
            wrap: WrapIndex::default(),
            monitor: None,
            data_rx: None,
            fstate: FileState::Missing,
            detail_applied: false,
            follow: true,
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
        self.follow = true;

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
    /// Idempotent. Returns immediately if the detail was already applied, or
    /// if the pane has been cleared and so has no job to watch a log for.
    pub fn set_detail(&mut self, detail: &JobDetail) {
        if self.detail_applied || self.job_id.is_none() {
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
                // Appending keeps every line already wrapped, so only a
                // replacement has to be reported to the wrap index.
                Ok(LogChunk::Append(text)) => self.content.push_str(&text),
                Ok(LogChunk::Replace(text)) => {
                    self.wrap.text_changed(&self.content, &text);
                    self.content = text;
                }
                Err(e) => {
                    let message = format!("Error watching file: {}", e);
                    self.wrap.text_changed(&self.content, &message);
                    self.content = message;
                    self.fstate = FileState::Failed;
                }
            }
        }
    }

    pub fn scroll_up(&mut self) {
        self.follow = false;
        self.scroll_pos = self.scroll_pos.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        if self.scroll_pos < self.max_scroll {
            self.scroll_pos += 1;
        }
        // Re-arm following once the user scrolls back to the bottom.
        self.follow = self.scroll_pos >= self.max_scroll;
    }

    pub fn page_up(&mut self) {
        self.follow = false;
        self.scroll_pos = self.scroll_pos.saturating_sub(10);
    }

    pub fn page_down(&mut self) {
        self.scroll_pos = (self.scroll_pos + 10).min(self.max_scroll);
        self.follow = self.scroll_pos >= self.max_scroll;
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
        self.follow = true;
        if let Some(m) = &mut self.monitor {
            m.set_file_path(None);
        }
    }

    pub fn render_inline(&mut self, frame: &mut Frame, area: Rect, focused: bool) {
        let focused_color = match self.stream {
            StreamKind::Stdout => ACCENT_STDOUT,
            StreamKind::Stderr => ACCENT_STDERR,
        };
        let border_color = if focused { focused_color } else { DIM_BORDER };

        let title = if self.job_id.is_some() && self.follow {
            format!(" {} [follow] ", self.stream.label())
        } else {
            format!(" {} ", self.stream.label())
        };
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(if focused {
                BorderType::Double
            } else {
                BorderType::Rounded
            })
            .border_style(Style::default().fg(border_color));

        if self.job_id.is_none() {
            let placeholder = Paragraph::new("Select a job to view output logs")
                .style(Style::default().fg(Color::DarkGray))
                .block(block);
            frame.render_widget(placeholder, area);
            return;
        }

        let placeholder = match self.fstate {
            FileState::Loading => Some(format!(
                "Loading {} details for job {}...",
                self.stream.label(),
                self.job_id.as_deref().unwrap_or("unknown")
            )),
            FileState::Missing => Some(format!(
                "No {} log file found for job {}",
                self.stream.label(),
                self.job_id.as_deref().unwrap_or("unknown")
            )),
            _ if self.content.is_empty() => {
                Some(format!("Waiting for {} content...", self.stream.label()))
            }
            _ => None,
        };

        let para = if let Some(text) = placeholder {
            self.max_scroll = 0;
            Paragraph::new(text)
        } else {
            let inner_width = area.width.saturating_sub(2);
            let inner_height = area.height.saturating_sub(2) as usize;
            self.max_scroll = self
                .wrap
                .rows(&self.content, inner_width)
                .saturating_sub(inner_height);
            if self.follow {
                self.scroll_pos = self.max_scroll;
            }
            let (visible, skip) = self
                .wrap
                .window(&self.content, self.scroll_pos, inner_height);
            Paragraph::new(visible).scroll((skip, 0))
        };

        frame.render_widget(
            para.style(Style::default().fg(Color::Rgb(200, 200, 210)))
                .block(block)
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (_, KeyCode::Up) => self.scroll_up(),
            (_, KeyCode::Down) => self.scroll_down(),
            (_, KeyCode::PageUp) | (KeyModifiers::CONTROL, KeyCode::Char('u')) => self.page_up(),
            (_, KeyCode::PageDown) | (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                self.page_down()
            }
            (_, KeyCode::Home) => {
                self.follow = false;
                self.scroll_pos = 0;
            }
            (_, KeyCode::End) => self.follow = true,
            (_, KeyCode::Char('f')) => self.follow = !self.follow,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    use std::{fs, io::Write, thread, time::Instant};

    fn wait_for_content(widget: &mut OutputWidget, want: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while widget.content != want && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
            widget.poll_updates();
        }
        assert_eq!(widget.content, want);
    }

    #[test]
    fn a_cleared_pane_stops_watching_its_log() {
        let path = std::env::temp_dir().join(format!("sqwatch-pane-{}.log", std::process::id()));
        fs::write(&path, "before\n").unwrap();
        let detail = JobDetail {
            stdout_file: Some(path.to_string_lossy().into_owned()),
            stderr_file: None,
            command: None,
            work_dir: None,
        };

        let mut widget = OutputWidget::new_for(StreamKind::Stdout);
        widget.ensure_job("1");
        widget.set_detail(&detail);
        wait_for_content(&mut widget, "before\n");

        // What the dashboard does to a hidden pane on every frame and tick.
        widget.clear_job();
        widget.set_detail(&detail);

        let mut log = fs::OpenOptions::new().append(true).open(&path).unwrap();
        log.write_all(b"after\n").unwrap();
        thread::sleep(POLL_INTERVAL + Duration::from_millis(500));
        let _ = fs::remove_file(&path);

        let queued: Vec<LogChunk> = widget
            .data_rx
            .as_ref()
            .unwrap()
            .try_iter()
            .filter_map(Result::ok)
            .collect();
        assert!(
            queued.iter().all(|chunk| match chunk {
                LogChunk::Replace(text) | LogChunk::Append(text) => !text.contains("after"),
            }),
            "the hidden pane still read the log: {:?}",
            queued
        );
    }

    #[test]
    fn follow_keeps_the_tail_past_the_u16_scroll_limit() {
        let mut w = OutputWidget::new_for(StreamKind::Stdout);
        w.job_id = Some("1".into());
        w.fstate = FileState::Pending;
        w.content = (1..=70_000)
            .map(|i| format!("line {:06}", i))
            .collect::<Vec<_>>()
            .join("\n");

        let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
        terminal
            .draw(|f| w.render_inline(f, f.area(), true))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let last_row: String = (1..39).map(|x| buffer[(x, 8)].symbol()).collect();
        assert_eq!(last_row.trim_end(), "line 070000");
    }
}
