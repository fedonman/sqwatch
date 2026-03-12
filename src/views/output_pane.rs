use color_eyre::Result;
use crossbeam::channel::{Receiver, unbounded};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::{collections::HashMap, iter::once, path::PathBuf, process::Command, time::Duration};

use crate::core::live_file::{LiveFileMonitor, MonitorError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    Stdout,
    Stderr,
}

impl StreamKind {
    fn swap(&mut self) {
        *self = match self {
            StreamKind::Stdout => StreamKind::Stderr,
            StreamKind::Stderr => StreamKind::Stdout,
        };
    }

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

pub struct OutputPane {
    pub visible: bool,
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

impl OutputPane {
    pub fn new() -> Self {
        Self {
            visible: false,
            job_id: None,
            stream: StreamKind::Stdout,
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

    pub fn show(&mut self, job_id: String) {
        self.switch_job(job_id);
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
        if let Some(m) = &mut self.monitor {
            m.set_file_path(None);
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

    pub fn toggle_stream(&mut self) {
        self.stream.swap();
        self.scroll_pos = 0;
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

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        frame.render_widget(Clear, area);

        let heading = match &self.job_id {
            Some(id) => format!("Job {} - {}", id, self.stream.label()),
            None => format!("Log View - {}", self.stream.label()),
        };

        let keys = " [\u{2191}/\u{2193}] Scroll | [Shift+\u{2191}/\u{2193}] Toggle Job | [o] Toggle stdout/stderr | [Esc] Close ";

        let display_text = match (self.fstate, self.content.is_empty()) {
            (FileState::Missing, _) => format!(
                "No {} log file found for job {}",
                self.stream.label(),
                self.job_id.as_deref().unwrap_or("unknown")
            ),
            _ => self.content.clone(),
        };

        let fitted = Self::prepare_text(
            &display_text,
            area.height as usize,
            area.width as usize,
            self.scroll_pos,
            false,
        );

        let widget = Paragraph::new(fitted)
            .style(Style::default().fg(Color::White))
            .block(
                Block::default()
                    .title(format!("{}{}", heading, keys))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: false })
            .scroll((self.scroll_pos as u16, 0));

        frame.render_widget(widget, area);
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (_, KeyCode::Char('o')) => self.toggle_stream(),
            (_, KeyCode::Up) => self.scroll_up(),
            (_, KeyCode::Down) => self.scroll_down(),
            (_, KeyCode::PageUp) | (KeyModifiers::CONTROL, KeyCode::Char('u')) => self.page_up(),
            (_, KeyCode::PageDown) | (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                self.page_down()
            }
            _ => {}
        }
    }

    fn prepare_text(raw: &str, rows: usize, cols: usize, offset: usize, _wrap: bool) -> Text<'_> {
        let cleaned: Vec<String> = raw
            .lines()
            .map(|line| {
                if line.contains('\r') {
                    line.split('\r').next_back().unwrap_or("").to_string()
                } else {
                    line.to_string()
                }
            })
            .collect();

        let joined = cleaned.join("\n");

        let rendered: Vec<Line> = joined
            .lines()
            .rev()
            .skip(offset)
            .flat_map(|l| {
                let chunks = Self::split_long_line(l, cols, cols.saturating_sub(2));
                chunks
                    .into_iter()
                    .enumerate()
                    .map(|(i, piece)| {
                        if i == 0 {
                            Line::raw(piece.to_string())
                        } else {
                            Line::default().spans(vec![
                                Span::styled(
                                    "\u{21aa} ",
                                    Style::default().add_modifier(Modifier::DIM),
                                ),
                                Span::raw(piece.to_string()),
                            ])
                        }
                    })
                    .rev()
            })
            .take(rows)
            .collect();

        Text::from(rendered.into_iter().rev().collect::<Vec<Line>>())
    }

    fn split_long_line(s: &str, first_width: usize, rest_width: usize) -> Vec<&str> {
        let breakpoints: Vec<usize> = s
            .char_indices()
            .map(|(i, _)| i)
            .enumerate()
            .filter(|&(n, _)| {
                if n > first_width {
                    rest_width > 0 && (n - first_width).is_multiple_of(rest_width)
                } else {
                    n == 0 || n == first_width
                }
            })
            .map(|(_, byte_pos)| byte_pos)
            .collect();

        let windows = breakpoints.windows(2).collect::<Vec<_>>();
        let tail_start = *breakpoints.last().unwrap_or(&0);

        windows
            .iter()
            .map(|w| &s[w[0]..w[1]])
            .chain(once(&s[tail_start..]))
            .collect()
    }

    fn resolve_log_paths(&mut self) {
        let Some(id) = &self.job_id else {
            self.fstate = FileState::Missing;
            return;
        };

        let result = Command::new("scontrol")
            .args(["show", "job", id, "-o"])
            .output();

        let Ok(output) = result else {
            self.fstate = FileState::Failed;
            return;
        };

        if !output.status.success() {
            self.fstate = FileState::Failed;
            return;
        }

        let raw = String::from_utf8_lossy(&output.stdout);
        let kv = parse_kv(&raw);

        self.stdout_file = kv.get("StdOut").cloned();
        self.stderr_file = kv.get("StdErr").cloned();

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

fn parse_kv(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for token in text.split_whitespace() {
        if let Some(eq) = token.find('=') {
            out.insert(token[..eq].to_string(), token[eq + 1..].to_string());
        }
    }
    out
}
