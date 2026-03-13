use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};
use regex::Regex;

use crate::backend::{JobState, query::QueryParams};

const ACCENT: Color = Color::Magenta;
const DIM_BORDER: Color = Color::Rgb(50, 50, 70);
const CHECKED_COLOR: Color = Color::Rgb(80, 200, 255);
const UNCHECKED_COLOR: Color = Color::Rgb(100, 100, 100);
const HEADER_COLOR: Color = Color::Rgb(180, 140, 220);
const INPUT_COLOR: Color = Color::Rgb(200, 180, 100);
const INVALID_COLOR: Color = Color::Rgb(230, 70, 70);

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
    Nodes,
    Partitions,
    QoS,
}

const SECTION_ORDER: [Section; 4] = [
    Section::States,
    Section::Nodes,
    Section::Partitions,
    Section::QoS,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    UserField,
    NameField,
    SectionItem,
}

pub struct FilterTree {
    pub open: bool,
    focus: Focus,
    /// Index into SECTION_ORDER
    section_idx: usize,
    /// Cursor within the current section's item list
    item_idx: usize,
    // Text input state
    user_input: String,
    name_input: String,
    editing: bool,
    user_ok: Option<bool>,
    name_ok: Option<bool>,
}

impl FilterTree {
    pub fn new() -> Self {
        Self {
            open: false,
            focus: Focus::UserField,
            section_idx: 0,
            item_idx: 0,
            user_input: String::new(),
            name_input: String::new(),
            editing: false,
            user_ok: None,
            name_ok: None,
        }
    }

    pub fn toggle(&mut self) -> bool {
        self.open = !self.open;
        if !self.open {
            self.editing = false;
        }
        self.open
    }

    /// Sync text inputs from params (call when opening sidebar).
    pub fn sync_from_params(&mut self, params: &QueryParams) {
        self.user_input = params.user.clone().unwrap_or_default();
        self.name_input = params.name_pattern.clone().unwrap_or_default();
        self.validate_user();
        self.validate_name();
    }

    pub fn is_editing(&self) -> bool {
        self.editing
    }

    fn validate_user(&mut self) {
        self.user_ok = if self.user_input.is_empty() {
            None
        } else {
            Some(Regex::new(&self.user_input).is_ok())
        };
    }

    fn validate_name(&mut self) {
        self.name_ok = if self.name_input.is_empty() {
            None
        } else {
            Some(Regex::new(&self.name_input).is_ok())
        };
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
            Section::Nodes => known_nodes.len(),
            Section::Partitions => known_partitions.len(),
            Section::QoS => known_qos.len(),
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
        if self.editing {
            return self.on_text_input(key, params);
        }

        match key.code {
            KeyCode::Up => {
                match self.focus {
                    Focus::UserField => {}
                    Focus::NameField => self.focus = Focus::UserField,
                    Focus::SectionItem => {
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
                        } else {
                            self.focus = Focus::NameField;
                        }
                    }
                }
                FilterTreeAction::Noop
            }
            KeyCode::Down => {
                match self.focus {
                    Focus::UserField => self.focus = Focus::NameField,
                    Focus::NameField => {
                        self.focus = Focus::SectionItem;
                        self.section_idx = 0;
                        self.item_idx = 0;
                    }
                    Focus::SectionItem => {
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
                    }
                }
                FilterTreeAction::Noop
            }
            KeyCode::Enter => match self.focus {
                Focus::UserField | Focus::NameField => {
                    self.editing = true;
                    FilterTreeAction::Noop
                }
                Focus::SectionItem => {
                    self.toggle_current(params, known_states, known_partitions, known_qos, known_nodes)
                }
            },
            KeyCode::Char(' ') => {
                if self.focus == Focus::SectionItem {
                    self.toggle_current(params, known_states, known_partitions, known_qos, known_nodes)
                } else {
                    FilterTreeAction::Noop
                }
            }
            KeyCode::Char('r') => {
                params.statuses.clear();
                params.partitions.clear();
                params.qos.clear();
                params.nodes.clear();
                params.user = None;
                params.name_pattern = None;
                self.user_input.clear();
                self.name_input.clear();
                self.user_ok = None;
                self.name_ok = None;
                FilterTreeAction::Applied
            }
            _ => FilterTreeAction::Noop,
        }
    }

    fn on_text_input(&mut self, key: KeyEvent, params: &mut QueryParams) -> FilterTreeAction {
        match key.code {
            KeyCode::Enter => {
                match self.focus {
                    Focus::UserField => {
                        if self.user_input.is_empty() {
                            params.user = None;
                        } else if self.user_ok == Some(true) {
                            params.user = Some(self.user_input.clone());
                        }
                    }
                    Focus::NameField => {
                        if self.name_input.is_empty() {
                            params.name_pattern = None;
                        } else if self.name_ok == Some(true) {
                            params.name_pattern = Some(self.name_input.clone());
                        }
                    }
                    _ => {}
                }
                self.editing = false;
                FilterTreeAction::Applied
            }
            KeyCode::Esc => {
                self.sync_from_params(params);
                self.editing = false;
                FilterTreeAction::Noop
            }
            KeyCode::Char(ch) => {
                match self.focus {
                    Focus::UserField => {
                        self.user_input.push(ch);
                        self.validate_user();
                    }
                    Focus::NameField => {
                        self.name_input.push(ch);
                        self.validate_name();
                    }
                    _ => {}
                }
                FilterTreeAction::Noop
            }
            KeyCode::Backspace => {
                match self.focus {
                    Focus::UserField => {
                        self.user_input.pop();
                        self.validate_user();
                    }
                    Focus::NameField => {
                        self.name_input.pop();
                        self.validate_name();
                    }
                    _ => {}
                }
                FilterTreeAction::Noop
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

        let max_val_w = (inner.width as usize).saturating_sub(6);
        let mut lines: Vec<Line> = Vec::new();

        // ── Text fields at the top ──
        lines.push(self.render_text_field(
            "usr",
            &self.user_input,
            self.user_ok,
            focused && self.focus == Focus::UserField,
            max_val_w,
        ));
        lines.push(self.render_text_field(
            "job",
            &self.name_input,
            self.name_ok,
            focused && self.focus == Focus::NameField,
            max_val_w,
        ));
        lines.push(Line::raw(""));

        // ── Section items ──
        for (si, sec) in SECTION_ORDER.iter().enumerate() {
            let header = match sec {
                Section::States => "States",
                Section::Nodes => "Nodes",
                Section::Partitions => "Partitions",
                Section::QoS => "QoS",
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

            let items = self.section_items_checked(*sec, params, known_states, known_partitions, known_qos, known_nodes);
            for (ii, (label, checked)) in items.iter().enumerate() {
                let is_cursor = focused && self.focus == Focus::SectionItem && si == self.section_idx && ii == self.item_idx;
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
        let cursor_line = self.cursor_line(known_states, known_partitions, known_qos, known_nodes);
        let visible_height = inner.height as usize;
        let scroll = if cursor_line >= visible_height {
            cursor_line - visible_height + 1
        } else {
            0
        };

        let widget = Paragraph::new(lines).scroll((scroll as u16, 0));
        frame.render_widget(widget, inner);

        // Show blinking cursor when editing a text field
        if focused && self.editing {
            let (input, line_idx) = match self.focus {
                Focus::UserField => (&self.user_input, 0usize),
                Focus::NameField => (&self.name_input, 1usize),
                _ => return,
            };
            if line_idx >= scroll {
                let y = inner.y + (line_idx - scroll) as u16;
                let x = inner.x + 6 + input.len().min(max_val_w) as u16;
                if y < inner.y + inner.height && x < inner.x + inner.width {
                    frame.set_cursor_position(Position { x, y });
                }
            }
        }
    }

    fn render_text_field(
        &self,
        label: &str,
        value: &str,
        valid: Option<bool>,
        is_focused: bool,
        max_w: usize,
    ) -> Line<'static> {
        let display = truncate(value, max_w);
        let text = format!(" {}: {}", label, display);

        let style = if is_focused {
            if valid == Some(false) {
                Style::default().fg(INVALID_COLOR).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            }
        } else if valid == Some(false) {
            Style::default().fg(INVALID_COLOR)
        } else if !value.is_empty() {
            Style::default().fg(INPUT_COLOR)
        } else {
            Style::default().fg(UNCHECKED_COLOR)
        };

        Line::from(Span::styled(text, style))
    }

    fn cursor_line(
        &self,
        known_states: &[JobState],
        known_partitions: &[String],
        known_qos: &[String],
        known_nodes: &[String],
    ) -> usize {
        match self.focus {
            Focus::UserField => 0,
            Focus::NameField => 1,
            Focus::SectionItem => {
                // 2 text fields + 1 blank separator = offset 3
                let mut pos = 3;
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
        }
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
            Section::Nodes => known_nodes
                .iter()
                .map(|n| (n.clone(), params.nodes.contains(n)))
                .collect(),
            Section::Partitions => known_partitions
                .iter()
                .map(|p| (p.clone(), params.partitions.contains(p)))
                .collect(),
            Section::QoS => known_qos
                .iter()
                .map(|q| (q.clone(), params.qos.contains(q)))
                .collect(),
        }
    }

    fn checked_count(&self, sec: Section, params: &QueryParams) -> usize {
        match sec {
            Section::States => params.statuses.len(),
            Section::Nodes => params.nodes.len(),
            Section::Partitions => params.partitions.len(),
            Section::QoS => params.qos.len(),
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
