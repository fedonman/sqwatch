use crossterm::event::KeyModifiers;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use super::theme::{ACCENT, DIM_BORDER, POPUP_BG};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobField {
    Id,
    Name,
    User,
    State,
    Partition,
    QoS,
    Nodes,
    Node,
    CPUs,
    Time,
    Memory,
    Account,
    Priority,
    WorkDir,
    SubmitTime,
    StartTime,
    EndTime,
    PendReason,
}

impl JobField {
    pub fn heading(&self) -> &'static str {
        match self {
            JobField::Id => "ID",
            JobField::Name => "Name",
            JobField::User => "User",
            JobField::State => "State",
            JobField::Partition => "Partition",
            JobField::QoS => "QoS",
            JobField::Nodes => "Nodes",
            JobField::Node => "Node",
            JobField::CPUs => "CPUs",
            JobField::Time => "Time",
            JobField::Memory => "Memory",
            JobField::Account => "Account",
            JobField::Priority => "Priority",
            JobField::WorkDir => "WorkDir",
            JobField::SubmitTime => "Submit",
            JobField::StartTime => "Start",
            JobField::EndTime => "End",
            JobField::PendReason => "Reason",
        }
    }

    pub fn format_code(&self) -> &'static str {
        match self {
            JobField::Id => "%i",
            JobField::Name => "%j",
            JobField::User => "%u",
            JobField::State => "%T",
            JobField::Partition => "%P",
            JobField::QoS => "%q",
            JobField::Nodes => "%D",
            JobField::Node => "%N",
            JobField::CPUs => "%C",
            JobField::Time => "%M",
            JobField::Memory => "%m",
            JobField::Account => "%a",
            JobField::Priority => "%Q",
            JobField::WorkDir => "%Z",
            JobField::SubmitTime => "%V",
            JobField::StartTime => "%S",
            JobField::EndTime => "%e",
            JobField::PendReason => "%R",
        }
    }

    pub fn width_hint(&self) -> Constraint {
        match self {
            JobField::Id => Constraint::Percentage(6),
            JobField::Name => Constraint::Percentage(8),
            JobField::User => Constraint::Percentage(8),
            JobField::State => Constraint::Percentage(7),
            JobField::Partition => Constraint::Percentage(7),
            JobField::QoS => Constraint::Percentage(6),
            JobField::Nodes => Constraint::Percentage(4),
            JobField::Node => Constraint::Percentage(10),
            JobField::CPUs => Constraint::Percentage(4),
            JobField::Time => Constraint::Percentage(7),
            JobField::Memory => Constraint::Percentage(6),
            JobField::Account => Constraint::Percentage(7),
            JobField::Priority => Constraint::Percentage(6),
            JobField::WorkDir => Constraint::Percentage(18),
            JobField::SubmitTime => Constraint::Percentage(10),
            JobField::StartTime => Constraint::Percentage(10),
            JobField::EndTime => Constraint::Percentage(10),
            JobField::PendReason => Constraint::Percentage(15),
        }
    }

    pub fn enumerate() -> Vec<JobField> {
        vec![
            JobField::Id,
            JobField::Name,
            JobField::User,
            JobField::State,
            JobField::Partition,
            JobField::QoS,
            JobField::Nodes,
            JobField::Node,
            JobField::CPUs,
            JobField::Time,
            JobField::Memory,
            JobField::Account,
            JobField::Priority,
            JobField::WorkDir,
            JobField::SubmitTime,
            JobField::StartTime,
            JobField::EndTime,
            JobField::PendReason,
        ]
    }

    pub fn defaults() -> Vec<JobField> {
        vec![
            JobField::Id,
            JobField::Name,
            JobField::State,
            JobField::Time,
            JobField::User,
            JobField::Node,
            JobField::Partition,
            JobField::CPUs,
            JobField::Memory,
            JobField::WorkDir,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ordering {
    Asc,
    Desc,
}

impl Ordering {
    pub fn flip(&self) -> Self {
        match self {
            Ordering::Asc => Ordering::Desc,
            Ordering::Desc => Ordering::Asc,
        }
    }

    pub fn arrow(&self) -> &'static str {
        match self {
            Ordering::Asc => "\u{25b2}",
            Ordering::Desc => "\u{25bc}",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderedField {
    pub field: JobField,
    pub direction: Ordering,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldFocus {
    Pool,
    Active,
    SortList,
}

pub struct FieldSelector {
    pub focus: FieldFocus,
    pub pool_state: ListState,
    pub active_state: ListState,
    pub sort_state: ListState,
    pub pool: Vec<JobField>,
    pub active: Vec<JobField>,
    pub sort_list: Vec<OrderedField>,
    pub visible: bool,
}

impl FieldSelector {
    pub fn new(active: Vec<JobField>, sort_list: Vec<OrderedField>) -> Self {
        let mut pool = JobField::enumerate();
        pool.retain(|f| !active.contains(f));

        let mut pool_state = ListState::default();
        if !pool.is_empty() {
            pool_state.select(Some(0));
        }

        let mut active_state = ListState::default();
        if !active.is_empty() {
            active_state.select(Some(0));
        }

        let mut sort_state = ListState::default();
        if !sort_list.is_empty() {
            sort_state.select(Some(0));
        }

        Self {
            focus: FieldFocus::Active,
            pool_state,
            active_state,
            sort_state,
            pool,
            active,
            sort_list,
            visible: false,
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Clear, area);

        let outer = Block::default()
            .title(Line::from(" \u{25c6} Column Management \u{25c6} ").centered())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT))
            .style(Style::default().bg(POPUP_BG));
        frame.render_widget(outer.clone(), area);

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Min(5), Constraint::Length(3)])
            .split(area);

        self.draw_columns(frame, sections[0]);
        self.draw_hints(frame, sections[1]);
    }

    fn draw_columns(&mut self, frame: &mut Frame, area: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ])
            .split(area);

        // Pool (available)
        let pool_block = Block::default()
            .title("Available Columns")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if self.focus == FieldFocus::Pool {
                ACCENT
            } else {
                DIM_BORDER
            }));

        let pool_items: Vec<ListItem> = self
            .pool
            .iter()
            .map(|f| ListItem::new(f.heading()).style(Style::default().fg(Color::Rgb(140, 140, 140))))
            .collect();

        let pool_list = List::new(pool_items)
            .block(pool_block)
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::White),
            );

        frame.render_stateful_widget(pool_list, cols[0], &mut self.pool_state);

        // Active (selected)
        let active_block = Block::default()
            .title("Selected Columns")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if self.focus == FieldFocus::Active {
                ACCENT
            } else {
                DIM_BORDER
            }));

        let active_items: Vec<ListItem> = self
            .active
            .iter()
            .map(|f| ListItem::new(f.heading()).style(Style::default().fg(Color::Rgb(140, 140, 140))))
            .collect();

        let active_list = List::new(active_items)
            .block(active_block)
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::White),
            );

        frame.render_stateful_widget(active_list, cols[1], &mut self.active_state);

        // Sort
        let sort_block = Block::default()
            .title("Sort Order")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if self.focus == FieldFocus::SortList {
                ACCENT
            } else {
                DIM_BORDER
            }));

        let sort_items: Vec<ListItem> = self
            .sort_list
            .iter()
            .map(|of| {
                ListItem::new(format!("{} {}", of.field.heading(), of.direction.arrow()))
                    .style(Style::default().fg(Color::Rgb(140, 140, 140)))
            })
            .collect();

        let sort_list_widget = List::new(sort_items)
            .block(sort_block)
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::White),
            );

        frame.render_stateful_widget(sort_list_widget, cols[2], &mut self.sort_state);
    }

    fn draw_hints(&self, frame: &mut Frame, area: Rect) {
        let hint = match self.focus {
            FieldFocus::Pool => {
                "\u{2191}/\u{2193}: Navigate | \u{2190}/\u{2192}: Switch lists | Enter: Add to Selected"
            }
            FieldFocus::Active => {
                "\u{2191}/\u{2193}: Navigate | \u{2190}/\u{2192}: Switch lists | Enter: Add to Sort | Del: Remove | Shift+\u{2191}/\u{2193}: Move"
            }
            FieldFocus::SortList => {
                "\u{2191}/\u{2193}: Navigate | \u{2190}/\u{2192}: Switch lists | Enter: Toggle order | Del: Remove | Shift+\u{2191}/\u{2193}: Move"
            }
        };

        let full = format!("{} | r: Reset | Ctrl+S: Save | Esc: Close", hint);
        let widget = Paragraph::new(full)
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(DIM_BORDER)),
            );
        frame.render_widget(widget, area);
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> FieldAction {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => return FieldAction::Dismiss,
            KeyCode::Tab => {
                self.rotate_focus();
                return FieldAction::Noop;
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return FieldAction::Save;
            }
            KeyCode::Char('r') => {
                self.reset_to_defaults();
                return FieldAction::Confirm;
            }
            KeyCode::Left => match self.focus {
                FieldFocus::Active => {
                    self.focus = FieldFocus::Pool;
                    self.ensure_selection();
                    return FieldAction::Noop;
                }
                FieldFocus::SortList => {
                    self.focus = FieldFocus::Active;
                    self.ensure_selection();
                    return FieldAction::Noop;
                }
                _ => {}
            },
            KeyCode::Right => match self.focus {
                FieldFocus::Pool => {
                    self.focus = FieldFocus::Active;
                    self.ensure_selection();
                    return FieldAction::Noop;
                }
                FieldFocus::Active => {
                    self.focus = FieldFocus::SortList;
                    self.ensure_selection();
                    return FieldAction::Noop;
                }
                _ => {}
            },
            _ => {}
        }

        match self.focus {
            FieldFocus::Pool => self.on_pool_key(key),
            FieldFocus::Active => self.on_active_key(key),
            FieldFocus::SortList => self.on_sort_key(key),
        }
    }

    fn on_pool_key(&mut self, key: crossterm::event::KeyEvent) -> FieldAction {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Up => {
                if let Some(cur) = self.pool_state.selected()
                    && cur > 0
                {
                    self.pool_state.select(Some(cur - 1));
                }
                FieldAction::Noop
            }
            KeyCode::Down => {
                if let Some(cur) = self.pool_state.selected()
                    && cur < self.pool.len().saturating_sub(1)
                {
                    self.pool_state.select(Some(cur + 1));
                }
                FieldAction::Noop
            }
            KeyCode::Enter => {
                if let Some(cur) = self.pool_state.selected()
                    && !self.pool.is_empty()
                    && cur < self.pool.len()
                {
                    let field = self.pool.remove(cur);
                    self.active.push(field);

                    if self.pool.is_empty() {
                        self.pool_state.select(None);
                    } else if cur >= self.pool.len() {
                        self.pool_state.select(Some(self.pool.len() - 1));
                    }

                    self.active_state.select(Some(self.active.len() - 1));
                }
                FieldAction::Confirm
            }
            _ => FieldAction::Noop,
        }
    }

    fn on_active_key(&mut self, key: crossterm::event::KeyEvent) -> FieldAction {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyModifiers;

        match key.code {
            KeyCode::Up => {
                if let Some(cur) = self.active_state.selected()
                    && cur > 0
                {
                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                        self.active.swap(cur, cur - 1);
                        self.active_state.select(Some(cur - 1));
                        return FieldAction::Confirm;
                    }
                    self.active_state.select(Some(cur - 1));
                }
                FieldAction::Noop
            }
            KeyCode::Down => {
                if let Some(cur) = self.active_state.selected()
                    && cur < self.active.len().saturating_sub(1)
                {
                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                        self.active.swap(cur, cur + 1);
                        self.active_state.select(Some(cur + 1));
                        return FieldAction::Confirm;
                    }
                    self.active_state.select(Some(cur + 1));
                }
                FieldAction::Noop
            }
            KeyCode::Enter => {
                if let Some(cur) = self.active_state.selected()
                    && !self.active.is_empty()
                    && cur < self.active.len()
                {
                    let field = self.active[cur];
                    let already_sorting = self.sort_list.iter().any(|of| of.field == field);
                    if !already_sorting {
                        self.sort_list.push(OrderedField {
                            field,
                            direction: Ordering::Asc,
                        });
                        self.sort_state.select(Some(self.sort_list.len() - 1));
                    }
                }
                FieldAction::Confirm
            }
            KeyCode::Delete | KeyCode::Backspace => {
                if let Some(cur) = self.active_state.selected()
                    && !self.active.is_empty()
                    && cur < self.active.len()
                {
                    let removed = self.active.remove(cur);
                    self.pool.push(removed);
                    self.sort_list.retain(|of| of.field != removed);

                    if self.active.is_empty() {
                        self.active_state.select(None);
                    } else if cur >= self.active.len() {
                        self.active_state.select(Some(self.active.len() - 1));
                    }
                    self.pool_state.select(Some(self.pool.len() - 1));
                }
                FieldAction::Confirm
            }
            _ => FieldAction::Noop,
        }
    }

    fn on_sort_key(&mut self, key: crossterm::event::KeyEvent) -> FieldAction {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyModifiers;

        match key.code {
            KeyCode::Up => {
                if let Some(cur) = self.sort_state.selected()
                    && cur > 0
                {
                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                        self.sort_list.swap(cur, cur - 1);
                        self.sort_state.select(Some(cur - 1));
                        return FieldAction::Confirm;
                    }
                    self.sort_state.select(Some(cur - 1));
                }
                FieldAction::Noop
            }
            KeyCode::Down => {
                if let Some(cur) = self.sort_state.selected()
                    && cur < self.sort_list.len().saturating_sub(1)
                {
                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                        self.sort_list.swap(cur, cur + 1);
                        self.sort_state.select(Some(cur + 1));
                        return FieldAction::Confirm;
                    }
                    self.sort_state.select(Some(cur + 1));
                }
                FieldAction::Noop
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let Some(cur) = self.sort_state.selected()
                    && cur < self.sort_list.len()
                {
                    self.sort_list[cur].direction = self.sort_list[cur].direction.flip();
                }
                FieldAction::Confirm
            }
            KeyCode::Delete | KeyCode::Backspace => {
                if let Some(cur) = self.sort_state.selected()
                    && !self.sort_list.is_empty()
                    && cur < self.sort_list.len()
                {
                    self.sort_list.remove(cur);
                    if self.sort_list.is_empty() {
                        self.sort_state.select(None);
                    } else if cur >= self.sort_list.len() {
                        self.sort_state.select(Some(self.sort_list.len() - 1));
                    }
                }
                FieldAction::Confirm
            }
            _ => FieldAction::Noop,
        }
    }

    fn rotate_focus(&mut self) {
        self.focus = match self.focus {
            FieldFocus::Pool => FieldFocus::Active,
            FieldFocus::Active => FieldFocus::SortList,
            FieldFocus::SortList => FieldFocus::Pool,
        };
        self.ensure_selection();
    }

    fn reset_to_defaults(&mut self) {
        self.active = JobField::defaults();
        self.sort_list = vec![OrderedField {
            field: JobField::Id,
            direction: Ordering::Asc,
        }];

        let mut pool = JobField::enumerate();
        pool.retain(|f| !self.active.contains(f));
        self.pool = pool;

        self.pool_state
            .select(if self.pool.is_empty() { None } else { Some(0) });
        self.active_state.select(Some(0));
        self.sort_state.select(Some(0));
    }

    fn ensure_selection(&mut self) {
        if self.focus == FieldFocus::Pool
            && !self.pool.is_empty()
            && self.pool_state.selected().is_none()
        {
            self.pool_state.select(Some(0));
        } else if self.focus == FieldFocus::Active
            && !self.active.is_empty()
            && self.active_state.selected().is_none()
        {
            self.active_state.select(Some(0));
        } else if self.focus == FieldFocus::SortList
            && !self.sort_list.is_empty()
            && self.sort_state.selected().is_none()
        {
            self.sort_state.select(Some(0));
        }
    }
}

pub enum FieldAction {
    Noop,
    Dismiss,
    Confirm,
    Save,
}
