use ratatui::{
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};
use regex::Regex;

use crate::backend::{query::QueryParams, JobState};

pub struct SearchDialog {
    pub tab_idx: usize,
    pub user_input: String,
    pub editing: bool,
    pub focus: SearchFocus,
    pub status_cursor: ListState,
    pub partition_cursor: ListState,
    pub qos_cursor: ListState,
    pub name_pattern: String,
    pub node_pattern: String,
    pub name_ok: Option<bool>,
    pub node_ok: Option<bool>,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchFocus {
    Username,
    States,
    Partitions,
    QoS,
    NamePattern,
    NodePattern,
}

impl SearchDialog {
    pub fn new() -> Self {
        let mut sc = ListState::default();
        sc.select(Some(0));
        let mut pc = ListState::default();
        pc.select(Some(0));
        let mut qc = ListState::default();
        qc.select(Some(0));

        Self {
            tab_idx: 0,
            user_input: String::new(),
            editing: false,
            focus: SearchFocus::Username,
            status_cursor: sc,
            partition_cursor: pc,
            qos_cursor: qc,
            name_pattern: String::new(),
            node_pattern: String::new(),
            name_ok: None,
            node_ok: None,
            visible: false,
        }
    }

    pub fn load_from(&mut self, params: &QueryParams) {
        self.user_input = params.user.clone().unwrap_or_default();
        self.name_pattern = params.name_pattern.clone().unwrap_or_default();
        self.node_pattern = params.node_pattern.clone().unwrap_or_default();

        self.name_ok = if self.name_pattern.is_empty() {
            None
        } else {
            Some(Regex::new(&self.name_pattern).is_ok())
        };

        self.node_ok = if self.node_pattern.is_empty() {
            None
        } else {
            Some(Regex::new(&self.node_pattern).is_ok())
        };
    }

    fn check_name_regex(&mut self) {
        if self.name_pattern.is_empty() {
            self.name_ok = None;
        } else {
            self.name_ok = Some(Regex::new(&self.name_pattern).is_ok());
        }
    }

    fn check_node_regex(&mut self) {
        if self.node_pattern.is_empty() {
            self.node_ok = None;
        } else {
            self.node_ok = Some(Regex::new(&self.node_pattern).is_ok());
        }
    }

    pub fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        params: &QueryParams,
        all_statuses: &[JobState],
        all_partitions: &[String],
        all_qos: &[String],
    ) {
        frame.render_widget(Clear, area);

        let outer = Block::default()
            .title(Line::from("Filter Jobs").centered())
            .borders(Borders::NONE)
            .style(Style::default().bg(Color::Black));
        frame.render_widget(outer.clone(), area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(8),
                Constraint::Min(5),
                Constraint::Length(3),
            ])
            .split(area);

        self.draw_text_inputs(frame, rows[0]);

        let triple = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
            ])
            .split(rows[1]);

        self.draw_status_list(frame, triple[0], params, all_statuses);
        self.draw_partition_list(frame, triple[1], params, all_partitions);
        self.draw_qos_list(frame, triple[2], params, all_qos);

        let hint = "\u{2191}/\u{2193}: Navigate | \u{2190}/\u{2192}: Switch Filters | Enter: Select/Apply | Esc: Close";
        let help = Paragraph::new(hint)
            .style(Style::default().fg(Color::Gray))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(help, rows[2]);
    }

    fn draw_text_inputs(&self, frame: &mut Frame, area: Rect) {
        let cells = Layout::default()
            .direction(Direction::Horizontal)
            .margin(1)
            .constraints([
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
            ])
            .split(area);

        // Username
        let u_block = Block::default()
            .title("Username")
            .borders(Borders::ALL)
            .style(if self.focus == SearchFocus::Username {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            });
        frame.render_widget(
            Paragraph::new(self.user_input.clone()).block(u_block),
            cells[0],
        );

        // Name pattern
        let n_title = match self.name_ok {
            Some(true) => "Job Name Filter (regex) \u{2713}",
            Some(false) => "Job Name Filter (regex) \u{2717} Invalid",
            None => "Job Name Filter (regex)",
        };
        let n_style = match (self.focus == SearchFocus::NamePattern, self.name_ok) {
            (true, _) => Style::default().fg(Color::Cyan),
            (_, Some(false)) => Style::default().fg(Color::Red),
            _ => Style::default(),
        };
        let n_block = Block::default()
            .title(n_title)
            .borders(Borders::ALL)
            .style(n_style);
        frame.render_widget(
            Paragraph::new(self.name_pattern.clone()).block(n_block),
            cells[1],
        );

        // Node pattern
        let nd_title = match self.node_ok {
            Some(true) => "Node Filter (regex) \u{2713}",
            Some(false) => "Node Filter (regex) \u{2717} Invalid",
            None => "Node Filter (regex)",
        };
        let nd_style = match (self.focus == SearchFocus::NodePattern, self.node_ok) {
            (true, _) => Style::default().fg(Color::Cyan),
            (_, Some(false)) => Style::default().fg(Color::Red),
            _ => Style::default(),
        };
        let nd_block = Block::default()
            .title(nd_title)
            .borders(Borders::ALL)
            .style(nd_style);
        frame.render_widget(
            Paragraph::new(self.node_pattern.clone()).block(nd_block),
            cells[2],
        );

        if self.editing {
            let (cx, cy) = match self.focus {
                SearchFocus::Username => (
                    cells[0].x + 1 + self.user_input.len() as u16,
                    cells[0].y + 1,
                ),
                SearchFocus::NamePattern => (
                    cells[1].x + 1 + self.name_pattern.len() as u16,
                    cells[1].y + 1,
                ),
                SearchFocus::NodePattern => (
                    cells[2].x + 1 + self.node_pattern.len() as u16,
                    cells[2].y + 1,
                ),
                _ => (0, 0),
            };
            if (cx, cy) != (0, 0) {
                frame.set_cursor_position(Position { x: cx, y: cy });
            }
        }
    }

    fn draw_status_list(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        params: &QueryParams,
        choices: &[JobState],
    ) {
        let block = Block::default()
            .title("Job States")
            .borders(Borders::ALL)
            .style(if self.focus == SearchFocus::States {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            });

        let items: Vec<ListItem> = choices
            .iter()
            .map(|st| {
                let on = params.statuses.contains(st);
                let mark = if on { "[X] " } else { "[ ] " };
                let c = if on { Color::Green } else { Color::White };
                ListItem::new(Line::from(format!("{}{}", mark, st))).style(Style::default().fg(c))
            })
            .collect();

        let widget = List::new(items)
            .block(block)
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));
        frame.render_stateful_widget(widget, area, &mut self.status_cursor);
    }

    fn draw_partition_list(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        params: &QueryParams,
        choices: &[String],
    ) {
        let block = Block::default()
            .title("Partitions")
            .borders(Borders::ALL)
            .style(if self.focus == SearchFocus::Partitions {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            });

        let items: Vec<ListItem> = choices
            .iter()
            .map(|p| {
                let on = params.partitions.contains(p);
                let mark = if on { "[X] " } else { "[ ] " };
                let c = if on { Color::Green } else { Color::White };
                ListItem::new(Line::from(format!("{}{}", mark, p))).style(Style::default().fg(c))
            })
            .collect();

        let widget = List::new(items)
            .block(block)
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));
        frame.render_stateful_widget(widget, area, &mut self.partition_cursor);
    }

    fn draw_qos_list(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        params: &QueryParams,
        choices: &[String],
    ) {
        let block = Block::default()
            .title("Quality of Service")
            .borders(Borders::ALL)
            .style(if self.focus == SearchFocus::QoS {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            });

        let items: Vec<ListItem> = choices
            .iter()
            .map(|q| {
                let on = params.qos.contains(q);
                let mark = if on { "[X] " } else { "[ ] " };
                let c = if on { Color::Green } else { Color::White };
                ListItem::new(Line::from(format!("{}{}", mark, q))).style(Style::default().fg(c))
            })
            .collect();

        let widget = List::new(items)
            .block(block)
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));
        frame.render_stateful_widget(widget, area, &mut self.qos_cursor);
    }

    pub fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        params: &mut QueryParams,
        all_statuses: &[JobState],
        all_partitions: &[String],
        all_qos: &[String],
    ) -> SearchAction {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => return SearchAction::Dismiss,
            KeyCode::Tab => {
                if self.editing {
                    self.editing = false;
                }
                return SearchAction::Noop;
            }
            KeyCode::F(10) => return SearchAction::Confirm,
            _ => {}
        }

        if self.editing {
            return self.on_text_input(key, params);
        }

        match key.code {
            KeyCode::Enter => match self.focus {
                SearchFocus::Username | SearchFocus::NamePattern | SearchFocus::NodePattern => {
                    self.editing = true;
                    SearchAction::Noop
                }
                SearchFocus::States => {
                    if let Some(idx) = self.status_cursor.selected() {
                        if idx < all_statuses.len() {
                            let st = all_statuses[idx];
                            if params.statuses.contains(&st) {
                                params.statuses.retain(|s| s != &st);
                            } else {
                                params.statuses.push(st);
                            }
                        }
                    }
                    SearchAction::Confirm
                }
                SearchFocus::Partitions => {
                    if let Some(idx) = self.partition_cursor.selected() {
                        if idx < all_partitions.len() {
                            let p = all_partitions[idx].clone();
                            if params.partitions.contains(&p) {
                                params.partitions.retain(|x| x != &p);
                            } else {
                                params.partitions.push(p);
                            }
                        }
                    }
                    SearchAction::Confirm
                }
                SearchFocus::QoS => {
                    if let Some(idx) = self.qos_cursor.selected() {
                        if idx < all_qos.len() {
                            let q = all_qos[idx].clone();
                            if params.qos.contains(&q) {
                                params.qos.retain(|x| x != &q);
                            } else {
                                params.qos.push(q);
                            }
                        }
                    }
                    SearchAction::Confirm
                }
            },
            KeyCode::Up => {
                self.navigate_list_up(all_statuses.len(), all_partitions.len(), all_qos.len());
                SearchAction::Noop
            }
            KeyCode::Down => {
                self.navigate_list_down(all_statuses.len(), all_partitions.len(), all_qos.len());
                SearchAction::Noop
            }
            KeyCode::Left => {
                if self.tab_idx == 0 {
                    self.tab_idx = 5;
                } else {
                    self.tab_idx -= 1;
                }
                self.sync_focus();
                SearchAction::Noop
            }
            KeyCode::Right => {
                if self.tab_idx == 5 {
                    self.tab_idx = 0;
                } else {
                    self.tab_idx += 1;
                }
                self.sync_focus();
                SearchAction::Noop
            }
            _ => SearchAction::Noop,
        }
    }

    fn navigate_list_up(&mut self, n_states: usize, n_parts: usize, n_qos: usize) {
        match self.focus {
            SearchFocus::States => {
                let cur = self.status_cursor.selected().unwrap_or(0);
                let next = if cur == 0 { n_states - 1 } else { cur - 1 };
                self.status_cursor.select(Some(next));
            }
            SearchFocus::Partitions => {
                let cur = self.partition_cursor.selected().unwrap_or(0);
                let next = if cur == 0 { n_parts - 1 } else { cur - 1 };
                self.partition_cursor.select(Some(next));
            }
            SearchFocus::QoS => {
                let cur = self.qos_cursor.selected().unwrap_or(0);
                let next = if cur == 0 { n_qos - 1 } else { cur - 1 };
                self.qos_cursor.select(Some(next));
            }
            _ => {}
        }
    }

    fn navigate_list_down(&mut self, n_states: usize, n_parts: usize, n_qos: usize) {
        match self.focus {
            SearchFocus::States => {
                let cur = self.status_cursor.selected().unwrap_or(0);
                let next = if cur >= n_states - 1 { 0 } else { cur + 1 };
                self.status_cursor.select(Some(next));
            }
            SearchFocus::Partitions => {
                let cur = self.partition_cursor.selected().unwrap_or(0);
                let next = if cur >= n_parts - 1 { 0 } else { cur + 1 };
                self.partition_cursor.select(Some(next));
            }
            SearchFocus::QoS => {
                let cur = self.qos_cursor.selected().unwrap_or(0);
                let next = if cur >= n_qos - 1 { 0 } else { cur + 1 };
                self.qos_cursor.select(Some(next));
            }
            _ => {}
        }
    }

    fn on_text_input(
        &mut self,
        key: crossterm::event::KeyEvent,
        params: &mut QueryParams,
    ) -> SearchAction {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Enter => {
                match self.focus {
                    SearchFocus::Username => {
                        params.user = if self.user_input.is_empty() {
                            None
                        } else {
                            Some(self.user_input.clone())
                        };
                    }
                    SearchFocus::NamePattern => {
                        if self.name_pattern.is_empty() {
                            params.name_pattern = None;
                            self.name_ok = None;
                        } else if self.name_ok == Some(true) {
                            params.name_pattern = Some(self.name_pattern.clone());
                        }
                    }
                    SearchFocus::NodePattern => {
                        if self.node_pattern.is_empty() {
                            params.node_pattern = None;
                            self.node_ok = None;
                        } else if self.node_ok == Some(true) {
                            params.node_pattern = Some(self.node_pattern.clone());
                        }
                    }
                    _ => {}
                }
                self.editing = false;
                SearchAction::Confirm
            }
            KeyCode::Char(ch) => {
                match self.focus {
                    SearchFocus::Username => self.user_input.push(ch),
                    SearchFocus::NamePattern => {
                        self.name_pattern.push(ch);
                        self.check_name_regex();
                    }
                    SearchFocus::NodePattern => {
                        self.node_pattern.push(ch);
                        self.check_node_regex();
                    }
                    _ => {}
                }
                SearchAction::Noop
            }
            KeyCode::Backspace => {
                match self.focus {
                    SearchFocus::Username => {
                        self.user_input.pop();
                    }
                    SearchFocus::NamePattern => {
                        self.name_pattern.pop();
                        self.check_name_regex();
                    }
                    SearchFocus::NodePattern => {
                        self.node_pattern.pop();
                        self.check_node_regex();
                    }
                    _ => {}
                }
                SearchAction::Noop
            }
            _ => SearchAction::Noop,
        }
    }

    fn sync_focus(&mut self) {
        self.focus = match self.tab_idx {
            0 => SearchFocus::Username,
            1 => SearchFocus::NamePattern,
            2 => SearchFocus::NodePattern,
            3 => SearchFocus::States,
            4 => SearchFocus::Partitions,
            5 => SearchFocus::QoS,
            _ => SearchFocus::Username,
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchAction {
    Noop,
    Dismiss,
    Confirm,
}
