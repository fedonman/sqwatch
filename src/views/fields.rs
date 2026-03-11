use crossterm::event::KeyModifiers;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

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
            JobField::Id => Constraint::Length(10),
            JobField::Name => Constraint::Percentage(20),
            JobField::User => Constraint::Length(10),
            JobField::State => Constraint::Length(12),
            JobField::Partition => Constraint::Length(12),
            JobField::QoS => Constraint::Length(10),
            JobField::Nodes => Constraint::Length(7),
            JobField::Node => Constraint::Percentage(12),
            JobField::CPUs => Constraint::Length(6),
            JobField::Time => Constraint::Length(12),
            JobField::Memory => Constraint::Length(10),
            JobField::Account => Constraint::Length(12),
            JobField::Priority => Constraint::Length(10),
            JobField::WorkDir => Constraint::Percentage(15),
            JobField::SubmitTime => Constraint::Length(19),
            JobField::StartTime => Constraint::Length(19),
            JobField::EndTime => Constraint::Length(19),
            JobField::PendReason => Constraint::Percentage(20),
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
            JobField::User,
            JobField::State,
            JobField::Time,
            JobField::Node,
            JobField::CPUs,
            JobField::Memory,
            JobField::Partition,
            JobField::QoS,
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
            Ordering::Asc => "\u{2191}",
            Ordering::Desc => "\u{2193}",
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
    BtnSave,
    BtnApply,
    BtnCancel,
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
            .title(Line::from("Column Management").centered())
            .borders(Borders::NONE)
            .style(Style::default().bg(Color::Black));
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
            .style(if self.focus == FieldFocus::Pool {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            });

        let pool_items: Vec<ListItem> = self
            .pool
            .iter()
            .map(|f| ListItem::new(f.heading()))
            .collect();

        let pool_list = List::new(pool_items)
            .block(pool_block)
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        frame.render_stateful_widget(pool_list, cols[0], &mut self.pool_state);

        // Active (selected)
        let active_block = Block::default()
            .title("Selected Columns")
            .borders(Borders::ALL)
            .style(if self.focus == FieldFocus::Active {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            });

        let active_items: Vec<ListItem> = self
            .active
            .iter()
            .map(|f| ListItem::new(f.heading()))
            .collect();

        let active_list = List::new(active_items)
            .block(active_block)
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        frame.render_stateful_widget(active_list, cols[1], &mut self.active_state);

        // Sort
        let sort_block = Block::default()
            .title("Sort Order")
            .borders(Borders::ALL)
            .style(if self.focus == FieldFocus::SortList {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            });

        let sort_items: Vec<ListItem> = self
            .sort_list
            .iter()
            .map(|of| {
                ListItem::new(format!("{} {}", of.field.heading(), of.direction.arrow()))
            })
            .collect();

        let sort_list_widget = List::new(sort_items)
            .block(sort_block)
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        frame.render_stateful_widget(sort_list_widget, cols[2], &mut self.sort_state);
    }

    fn draw_hints(&self, frame: &mut Frame, area: Rect) {
        let hint = match self.focus {
            FieldFocus::Pool => {
                "\u{2191}/\u{2193}: Navigate | \u{2190}/\u{2192}: Switch lists | Enter: Add to Selected"
            }
            FieldFocus::Active => {
                "\u{2191}/\u{2193}: Navigate | \u{2190}/\u{2192}: Switch lists | Enter: Add to Sort | Del: Remove | Ctrl+\u{2191}/\u{2193}: Move up/down"
            }
            FieldFocus::SortList => {
                "\u{2191}/\u{2193}: Navigate | \u{2190}/\u{2192}: Switch lists | Enter: Toggle order | Del: Remove | Ctrl+\u{2191}/\u{2193}: Move up/down"
            }
            _ => "",
        };

        let full = format!("{} | Ctrl+a: Apply | Esc: Close", hint);
        let widget = Paragraph::new(full)
            .style(Style::default().fg(Color::Gray))
            .block(Block::default().borders(Borders::ALL));
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
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return FieldAction::Confirm;
            }
            _ => {}
        }

        match self.focus {
            FieldFocus::Pool => self.on_pool_key(key),
            FieldFocus::Active => self.on_active_key(key),
            FieldFocus::SortList => self.on_sort_key(key),
            _ => self.on_button_key(key),
        }
    }

    fn on_pool_key(&mut self, key: crossterm::event::KeyEvent) -> FieldAction {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Up => {
                if let Some(cur) = self.pool_state.selected() {
                    if cur > 0 {
                        self.pool_state.select(Some(cur - 1));
                    }
                }
                FieldAction::Noop
            }
            KeyCode::Down => {
                if let Some(cur) = self.pool_state.selected() {
                    if cur < self.pool.len().saturating_sub(1) {
                        self.pool_state.select(Some(cur + 1));
                    }
                }
                FieldAction::Noop
            }
            KeyCode::Enter => {
                if let Some(cur) = self.pool_state.selected() {
                    if !self.pool.is_empty() && cur < self.pool.len() {
                        let field = self.pool.remove(cur);
                        self.active.push(field);

                        if self.pool.is_empty() {
                            self.pool_state.select(None);
                        } else if cur >= self.pool.len() {
                            self.pool_state.select(Some(self.pool.len() - 1));
                        }

                        self.active_state.select(Some(self.active.len() - 1));
                    }
                }
                FieldAction::Noop
            }
            _ => FieldAction::Noop,
        }
    }

    fn on_active_key(&mut self, key: crossterm::event::KeyEvent) -> FieldAction {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyModifiers;

        match key.code {
            KeyCode::Up => {
                if let Some(cur) = self.active_state.selected() {
                    if cur > 0 {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            self.active.swap(cur, cur - 1);
                        }
                        self.active_state.select(Some(cur - 1));
                    }
                }
                FieldAction::Noop
            }
            KeyCode::Down => {
                if let Some(cur) = self.active_state.selected() {
                    if cur < self.active.len().saturating_sub(1) {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            self.active.swap(cur, cur + 1);
                        }
                        self.active_state.select(Some(cur + 1));
                    }
                }
                FieldAction::Noop
            }
            KeyCode::Enter => {
                if let Some(cur) = self.active_state.selected() {
                    if !self.active.is_empty() && cur < self.active.len() {
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
                }
                FieldAction::Noop
            }
            KeyCode::Delete | KeyCode::Backspace => {
                if let Some(cur) = self.active_state.selected() {
                    if !self.active.is_empty() && cur < self.active.len() {
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
                }
                FieldAction::Noop
            }
            _ => FieldAction::Noop,
        }
    }

    fn on_sort_key(&mut self, key: crossterm::event::KeyEvent) -> FieldAction {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyModifiers;

        match key.code {
            KeyCode::Up => {
                if let Some(cur) = self.sort_state.selected() {
                    if cur > 0 {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            self.sort_list.swap(cur, cur - 1);
                        }
                        self.sort_state.select(Some(cur - 1));
                    }
                }
                FieldAction::Noop
            }
            KeyCode::Down => {
                if let Some(cur) = self.sort_state.selected() {
                    if cur < self.sort_list.len().saturating_sub(1) {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            self.sort_list.swap(cur, cur + 1);
                        }
                        self.sort_state.select(Some(cur + 1));
                    }
                }
                FieldAction::Noop
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let Some(cur) = self.sort_state.selected() {
                    if cur < self.sort_list.len() {
                        self.sort_list[cur].direction = self.sort_list[cur].direction.flip();
                    }
                }
                FieldAction::Noop
            }
            KeyCode::Delete | KeyCode::Backspace => {
                if let Some(cur) = self.sort_state.selected() {
                    if !self.sort_list.is_empty() && cur < self.sort_list.len() {
                        self.sort_list.remove(cur);
                        if self.sort_list.is_empty() {
                            self.sort_state.select(None);
                        } else if cur >= self.sort_list.len() {
                            self.sort_state.select(Some(self.sort_list.len() - 1));
                        }
                    }
                }
                FieldAction::Noop
            }
            _ => FieldAction::Noop,
        }
    }

    fn on_button_key(&mut self, key: crossterm::event::KeyEvent) -> FieldAction {
        use crossterm::event::KeyCode;

        match (self.focus, key.code) {
            (FieldFocus::BtnSave, KeyCode::Enter) => FieldAction::PersistAndConfirm,
            (FieldFocus::BtnApply, KeyCode::Enter) => FieldAction::Confirm,
            (FieldFocus::BtnCancel, KeyCode::Enter) => FieldAction::Dismiss,
            _ => FieldAction::Noop,
        }
    }

    fn rotate_focus(&mut self) {
        self.focus = match self.focus {
            FieldFocus::Pool => FieldFocus::Active,
            FieldFocus::Active => FieldFocus::SortList,
            FieldFocus::SortList => FieldFocus::BtnSave,
            FieldFocus::BtnSave => FieldFocus::BtnApply,
            FieldFocus::BtnApply => FieldFocus::BtnCancel,
            FieldFocus::BtnCancel => FieldFocus::Pool,
        };
        self.ensure_selection();
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
    PersistAndConfirm,
}
