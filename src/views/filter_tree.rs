use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::backend::{JobState, query::QueryParams};

const ACCENT: Color = Color::Magenta;
const DIM_BORDER: Color = Color::Rgb(50, 50, 70);
const CHECKED_COLOR: Color = Color::Rgb(80, 200, 255);
const UNCHECKED_COLOR: Color = Color::Rgb(100, 100, 100);
const HEADER_COLOR: Color = Color::Rgb(180, 140, 220);

/// Width of the sidebar when open.
pub const SIDEBAR_WIDTH: u16 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterTreeAction {
    Noop,
    Applied,
}

/// Identifies which section + item the cursor is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    States,
    Partitions,
    QoS,
    Nodes,
}

const SECTION_ORDER: [Section; 4] = [
    Section::States,
    Section::Partitions,
    Section::QoS,
    Section::Nodes,
];

pub struct FilterTree {
    pub open: bool,
    /// Index into SECTION_ORDER
    section_idx: usize,
    /// Cursor within the current section's item list
    item_idx: usize,
}

impl FilterTree {
    pub fn new() -> Self {
        Self {
            open: false,
            section_idx: 0,
            item_idx: 0,
        }
    }

    pub fn toggle(&mut self) -> bool {
        self.open = !self.open;
        self.open
    }

    fn section_len(
        &self,
        sec: Section,
        known_states: &[JobState],
        known_partitions: &[String],
        known_qos: &[String],
        known_nodes: &[String],
    ) -> usize {
        match sec {
            Section::States => known_states.len(),
            Section::Partitions => known_partitions.len(),
            Section::QoS => known_qos.len(),
            Section::Nodes => known_nodes.len(),
        }
    }

    fn current_section_len(
        &self,
        known_states: &[JobState],
        known_partitions: &[String],
        known_qos: &[String],
        known_nodes: &[String],
    ) -> usize {
        self.section_len(
            SECTION_ORDER[self.section_idx],
            known_states,
            known_partitions,
            known_qos,
            known_nodes,
        )
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        params: &mut QueryParams,
        known_states: &[JobState],
        known_partitions: &[String],
        known_qos: &[String],
        known_nodes: &[String],
    ) -> FilterTreeAction {
        match key.code {
            KeyCode::Up => {
                if self.item_idx > 0 {
                    self.item_idx -= 1;
                } else if self.section_idx > 0 {
                    self.section_idx -= 1;
                    let len = self.current_section_len(
                        known_states,
                        known_partitions,
                        known_qos,
                        known_nodes,
                    );
                    self.item_idx = len.saturating_sub(1);
                }
                FilterTreeAction::Noop
            }
            KeyCode::Down => {
                let len = self.current_section_len(
                    known_states,
                    known_partitions,
                    known_qos,
                    known_nodes,
                );
                if self.item_idx + 1 < len {
                    self.item_idx += 1;
                } else if self.section_idx + 1 < SECTION_ORDER.len() {
                    self.section_idx += 1;
                    self.item_idx = 0;
                }
                FilterTreeAction::Noop
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.toggle_current(params, known_states, known_partitions, known_qos, known_nodes)
            }
            KeyCode::Char('r') => {
                params.statuses.clear();
                params.partitions.clear();
                params.qos.clear();
                params.nodes.clear();
                params.user = None;
                params.name_pattern = None;
                FilterTreeAction::Applied
            }
            _ => FilterTreeAction::Noop,
        }
    }

    fn toggle_current(
        &self,
        params: &mut QueryParams,
        known_states: &[JobState],
        known_partitions: &[String],
        known_qos: &[String],
        known_nodes: &[String],
    ) -> FilterTreeAction {
        let sec = SECTION_ORDER[self.section_idx];
        let idx = self.item_idx;

        match sec {
            Section::States => {
                if idx < known_states.len() {
                    let st = known_states[idx];
                    if params.statuses.contains(&st) {
                        params.statuses.retain(|s| s != &st);
                    } else {
                        params.statuses.push(st);
                    }
                    return FilterTreeAction::Applied;
                }
            }
            Section::Partitions => {
                if idx < known_partitions.len() {
                    let p = &known_partitions[idx];
                    if params.partitions.contains(p) {
                        params.partitions.retain(|x| x != p);
                    } else {
                        params.partitions.push(p.clone());
                    }
                    return FilterTreeAction::Applied;
                }
            }
            Section::QoS => {
                if idx < known_qos.len() {
                    let q = &known_qos[idx];
                    if params.qos.contains(q) {
                        params.qos.retain(|x| x != q);
                    } else {
                        params.qos.push(q.clone());
                    }
                    return FilterTreeAction::Applied;
                }
            }
            Section::Nodes => {
                if idx < known_nodes.len() {
                    let n = &known_nodes[idx];
                    if params.nodes.contains(n) {
                        params.nodes.retain(|x| x != n);
                    } else {
                        params.nodes.push(n.clone());
                    }
                    return FilterTreeAction::Applied;
                }
            }
        }
        FilterTreeAction::Noop
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        focused: bool,
        params: &QueryParams,
        known_states: &[JobState],
        known_partitions: &[String],
        known_qos: &[String],
        known_nodes: &[String],
    ) {
        let border_color = if focused { ACCENT } else { DIM_BORDER };
        let block = Block::default()
            .title(" Filters ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        // Build flat list of lines
        let mut lines: Vec<Line> = Vec::new();
        let cursor_flat = self.flat_cursor_pos(known_states, known_partitions, known_qos, known_nodes);

        // Regex summary at top
        if let Some(ref u) = params.user {
            let label = format!(" usr: {}", truncate(u, inner.width as usize - 6));
            lines.push(Line::from(Span::styled(
                label,
                Style::default().fg(Color::Rgb(200, 180, 100)),
            )));
        }
        if let Some(ref n) = params.name_pattern {
            let label = format!(" job: {}", truncate(n, inner.width as usize - 6));
            lines.push(Line::from(Span::styled(
                label,
                Style::default().fg(Color::Rgb(200, 180, 100)),
            )));
        }
        if params.user.is_some() || params.name_pattern.is_some() {
            lines.push(Line::raw(""));
        }

        for (si, sec) in SECTION_ORDER.iter().enumerate() {
            // Section header
            let header = match sec {
                Section::States => "States",
                Section::Partitions => "Partitions",
                Section::QoS => "QoS",
                Section::Nodes => "Nodes",
            };
            let count = self.checked_count(*sec, params);
            let header_text = if count > 0 {
                format!("\u{25bc} {} ({})", header, count)
            } else {
                format!("\u{25bc} {}", header)
            };
            lines.push(Line::from(Span::styled(
                header_text,
                Style::default().fg(HEADER_COLOR).add_modifier(Modifier::BOLD),
            )));

            // Items
            let items = self.section_items_checked(*sec, params, known_states, known_partitions, known_qos, known_nodes);
            for (ii, (label, checked)) in items.iter().enumerate() {
                let is_cursor = focused && si == self.section_idx && ii == self.item_idx;
                let mark = if *checked { "\u{25c6}" } else { "\u{25c7}" };
                let color = if *checked { CHECKED_COLOR } else { UNCHECKED_COLOR };

                let item_label = truncate(label, inner.width as usize - 5);
                let text = format!("  {} {}", mark, item_label);

                let mut style = Style::default().fg(color);
                if is_cursor {
                    style = style
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD);
                }

                lines.push(Line::from(Span::styled(text, style)));
            }
        }

        // Scroll to keep cursor visible
        let visible_height = inner.height as usize;
        let scroll = if cursor_flat + self.regex_header_lines(params) >= visible_height {
            (cursor_flat + self.regex_header_lines(params)).saturating_sub(visible_height - 1)
        } else {
            0
        };

        let widget = Paragraph::new(lines).scroll((scroll as u16, 0));
        frame.render_widget(widget, inner);
    }

    fn regex_header_lines(&self, params: &QueryParams) -> usize {
        let mut n = 0;
        if params.user.is_some() {
            n += 1;
        }
        if params.name_pattern.is_some() {
            n += 1;
        }
        if n > 0 {
            n += 1; // blank separator
        }
        n
    }

    fn flat_cursor_pos(
        &self,
        known_states: &[JobState],
        known_partitions: &[String],
        known_qos: &[String],
        known_nodes: &[String],
    ) -> usize {
        let mut pos = 0;
        for sec in SECTION_ORDER.iter().take(self.section_idx) {
            pos += 1; // header
            pos += self.section_len(
                *sec,
                known_states,
                known_partitions,
                known_qos,
                known_nodes,
            );
        }
        pos += 1; // current section header
        pos += self.item_idx;
        pos
    }

    fn section_items_checked(
        &self,
        sec: Section,
        params: &QueryParams,
        known_states: &[JobState],
        known_partitions: &[String],
        known_qos: &[String],
        known_nodes: &[String],
    ) -> Vec<(String, bool)> {
        match sec {
            Section::States => known_states
                .iter()
                .map(|st| (st.to_string(), params.statuses.contains(st)))
                .collect(),
            Section::Partitions => known_partitions
                .iter()
                .map(|p| (p.clone(), params.partitions.contains(p)))
                .collect(),
            Section::QoS => known_qos
                .iter()
                .map(|q| (q.clone(), params.qos.contains(q)))
                .collect(),
            Section::Nodes => known_nodes
                .iter()
                .map(|n| (n.clone(), params.nodes.contains(n)))
                .collect(),
        }
    }

    fn checked_count(&self, sec: Section, params: &QueryParams) -> usize {
        match sec {
            Section::States => params.statuses.len(),
            Section::Partitions => params.partitions.len(),
            Section::QoS => params.qos.len(),
            Section::Nodes => params.nodes.len(),
        }
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else if max > 3 {
        &s[..max - 3]
    } else {
        &s[..max]
    }
}
