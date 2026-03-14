use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use serde::{Deserialize, Serialize};

use super::theme::{ACCENT, CHECKED_COLOR, DIM_BORDER, POPUP_BG, UNCHECKED_COLOR};

const CUSTOM_COLOR: Color = Color::Rgb(180, 130, 255);
const CUSTOM_DIM: Color = Color::Rgb(120, 100, 160);
const MAX_RIGHT_WIDGETS: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WidgetKind {
    Filters,
    Script,
    Stdout,
    Stderr,
    Custom(usize),
}

impl WidgetKind {
    pub fn label<'a>(&self, custom_defs: &'a [CustomWidgetDef]) -> &'a str {
        match self {
            WidgetKind::Filters => "Filters",
            WidgetKind::Script => "Execution Script",
            WidgetKind::Stdout => "stdout",
            WidgetKind::Stderr => "stderr",
            WidgetKind::Custom(i) => custom_defs
                .get(*i)
                .map(|d| d.title.as_str())
                .unwrap_or("Custom"),
        }
    }

}

const BUILTIN_ORDER: [WidgetKind; 4] = [
    WidgetKind::Filters,
    WidgetKind::Script,
    WidgetKind::Stdout,
    WidgetKind::Stderr,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomWidgetDef {
    pub title: String,
    pub filename: String,
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub struct VisibleWidgets {
    pub filters: bool,
    pub script: bool,
    pub stdout: bool,
    pub stderr: bool,
    pub custom: Vec<CustomWidgetDef>,
}

impl Default for VisibleWidgets {
    fn default() -> Self {
        Self {
            filters: false,
            script: true,
            stdout: true,
            stderr: false,
            custom: Vec::new(),
        }
    }
}

impl VisibleWidgets {
    pub fn is_visible(&self, kind: &WidgetKind) -> bool {
        match kind {
            WidgetKind::Filters => self.filters,
            WidgetKind::Script => self.script,
            WidgetKind::Stdout => self.stdout,
            WidgetKind::Stderr => self.stderr,
            WidgetKind::Custom(i) => self.custom.get(*i).is_some_and(|c| c.visible),
        }
    }

    pub fn toggle(&mut self, kind: &WidgetKind) {
        match kind {
            WidgetKind::Filters => self.filters = !self.filters,
            WidgetKind::Script => self.script = !self.script,
            WidgetKind::Stdout => self.stdout = !self.stdout,
            WidgetKind::Stderr => self.stderr = !self.stderr,
            WidgetKind::Custom(i) => {
                if let Some(c) = self.custom.get_mut(*i) {
                    c.visible = !c.visible;
                }
            }
        }
    }

    /// Count of visible right-panel widgets (excludes the sidebar).
    pub fn right_widget_count(&self) -> usize {
        let builtin = [self.script, self.stdout, self.stderr]
            .iter()
            .filter(|&&v| v)
            .count();
        let custom = self.custom.iter().filter(|c| c.visible).count();
        builtin + custom
    }

    /// Ordered list of visible right-panel widget kinds.
    pub fn visible_right_widgets(&self) -> Vec<WidgetKind> {
        let mut out = Vec::new();
        if self.script {
            out.push(WidgetKind::Script);
        }
        if self.stdout {
            out.push(WidgetKind::Stdout);
        }
        if self.stderr {
            out.push(WidgetKind::Stderr);
        }
        for (i, c) in self.custom.iter().enumerate() {
            if c.visible {
                out.push(WidgetKind::Custom(i));
            }
        }
        out
    }

    /// Full ordered list for the widget selector (all items including hidden).
    fn all_widget_kinds(&self) -> Vec<WidgetKind> {
        let mut out: Vec<WidgetKind> = BUILTIN_ORDER.to_vec();
        for i in 0..self.custom.len() {
            out.push(WidgetKind::Custom(i));
        }
        out
    }

    pub fn has_custom_widgets(&self) -> bool {
        !self.custom.is_empty()
    }

    pub fn remove_custom(&mut self, idx: usize) {
        if idx < self.custom.len() {
            self.custom.remove(idx);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetSelectorAction {
    Noop,
    Dismiss,
    Changed,
    Save,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddPhase {
    Title,
    Filename,
}

pub struct WidgetSelector {
    pub visible: bool,
    cursor: usize,
    adding: bool,
    add_phase: AddPhase,
    add_title_buf: String,
    add_filename_buf: String,
}

impl WidgetSelector {
    pub fn new() -> Self {
        Self {
            visible: false,
            cursor: 0,
            adding: false,
            add_phase: AddPhase::Title,
            add_title_buf: String::new(),
            add_filename_buf: String::new(),
        }
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        widgets: &mut VisibleWidgets,
    ) -> WidgetSelectorAction {
        if self.adding {
            return self.handle_add_key(key, widgets);
        }

        let all_kinds = widgets.all_widget_kinds();
        let item_count = all_kinds.len();

        match key.code {
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                WidgetSelectorAction::Save
            }
            KeyCode::Esc => WidgetSelectorAction::Dismiss,
            KeyCode::Up => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                WidgetSelectorAction::Noop
            }
            KeyCode::Down => {
                if self.cursor < item_count.saturating_sub(1) {
                    self.cursor += 1;
                }
                WidgetSelectorAction::Noop
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if self.cursor < item_count {
                    let kind = &all_kinds[self.cursor];
                    // Enforce cap: only block toggling ON, not OFF
                    if !widgets.is_visible(kind)
                        && *kind != WidgetKind::Filters
                        && widgets.right_widget_count() >= MAX_RIGHT_WIDGETS
                    {
                        return WidgetSelectorAction::Noop;
                    }
                    widgets.toggle(kind);
                    WidgetSelectorAction::Changed
                } else {
                    WidgetSelectorAction::Noop
                }
            }
            KeyCode::Char('a') => {
                self.adding = true;
                self.add_phase = AddPhase::Title;
                self.add_title_buf.clear();
                self.add_filename_buf.clear();
                WidgetSelectorAction::Noop
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if self.cursor < item_count
                    && let WidgetKind::Custom(i) = &all_kinds[self.cursor]
                {
                    widgets.remove_custom(*i);
                    if self.cursor > 0 && self.cursor >= item_count - 1 {
                        self.cursor -= 1;
                    }
                    return WidgetSelectorAction::Changed;
                }
                WidgetSelectorAction::Noop
            }
            _ => WidgetSelectorAction::Noop,
        }
    }

    fn handle_add_key(
        &mut self,
        key: KeyEvent,
        widgets: &mut VisibleWidgets,
    ) -> WidgetSelectorAction {
        match key.code {
            KeyCode::Esc => {
                self.adding = false;
                WidgetSelectorAction::Noop
            }
            KeyCode::Enter => match self.add_phase {
                AddPhase::Title => {
                    if !self.add_title_buf.trim().is_empty() {
                        self.add_phase = AddPhase::Filename;
                    }
                    WidgetSelectorAction::Noop
                }
                AddPhase::Filename => {
                    if !self.add_filename_buf.trim().is_empty() {
                        widgets.custom.push(CustomWidgetDef {
                            title: self.add_title_buf.trim().to_string(),
                            filename: self.add_filename_buf.trim().to_string(),
                            visible: true,
                        });
                        self.adding = false;
                        self.cursor = BUILTIN_ORDER.len() + widgets.custom.len() - 1;
                        WidgetSelectorAction::Changed
                    } else {
                        WidgetSelectorAction::Noop
                    }
                }
            },
            KeyCode::Backspace => {
                match self.add_phase {
                    AddPhase::Title => {
                        self.add_title_buf.pop();
                    }
                    AddPhase::Filename => {
                        self.add_filename_buf.pop();
                    }
                }
                WidgetSelectorAction::Noop
            }
            KeyCode::Char(ch) => {
                match self.add_phase {
                    AddPhase::Title => self.add_title_buf.push(ch),
                    AddPhase::Filename => self.add_filename_buf.push(ch),
                }
                WidgetSelectorAction::Noop
            }
            _ => WidgetSelectorAction::Noop,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, widgets: &VisibleWidgets) {
        frame.render_widget(Clear, area);

        let outer = Block::default()
            .title(Line::from(" \u{25c6} Widget Layout \u{25c6} ").centered())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT))
            .style(Style::default().bg(POPUP_BG));

        let inner = outer.inner(area);
        frame.render_widget(outer, area);

        if inner.height < 2 || inner.width < 4 {
            return;
        }

        if self.adding {
            self.render_add_form(frame, inner);
            return;
        }

        let all_kinds = widgets.all_widget_kinds();
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::raw(""));

        for (i, kind) in all_kinds.iter().enumerate() {
            // Separator before custom widgets section
            if i == BUILTIN_ORDER.len() && widgets.has_custom_widgets() {
                let sep = format!(
                    "  {:\u{2500}<width$}",
                    "\u{2500} Custom Widgets ",
                    width = inner.width.saturating_sub(4) as usize
                );
                lines.push(Line::from(Span::styled(
                    sep,
                    Style::default().fg(DIM_BORDER),
                )));
            }

            let checked = widgets.is_visible(kind);
            let is_cursor = i == self.cursor;
            let is_custom = matches!(kind, WidgetKind::Custom(_));

            let mark = if is_custom {
                if checked { "\u{25cf}" } else { "\u{25cb}" }
            } else if checked {
                "\u{25c6}"
            } else {
                "\u{25c7}"
            };

            let label = kind.label(&widgets.custom);
            let text = format!("  {} {}", mark, label);

            let base_color = if is_custom {
                if checked { CUSTOM_COLOR } else { CUSTOM_DIM }
            } else if checked {
                CHECKED_COLOR
            } else {
                UNCHECKED_COLOR
            };

            let mut style = Style::default().fg(base_color);
            if is_cursor {
                style = style.fg(Color::White).add_modifier(Modifier::BOLD);
            }

            lines.push(Line::from(Span::styled(text, style)));
        }

        lines.push(Line::raw(""));

        let hint =
            " \u{2191}\u{2193}: Navigate | Enter: Toggle | a: Add | d: Delete | Ctrl+S: Save | Esc: Close";
        lines.push(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )));

        let widget = Paragraph::new(lines);
        frame.render_widget(widget, inner);
    }

    fn render_add_form(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  Add Custom Widget",
            Style::default()
                .fg(CUSTOM_COLOR)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::raw(""));

        // Title field
        let title_label_style = if self.add_phase == AddPhase::Title {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(UNCHECKED_COLOR)
        };
        let title_value = if self.add_phase == AddPhase::Title {
            format!("{}|", self.add_title_buf)
        } else {
            self.add_title_buf.clone()
        };
        lines.push(Line::from(vec![
            Span::styled("  Title:    ", title_label_style),
            Span::styled(title_value, Style::default().fg(Color::White)),
        ]));

        lines.push(Line::raw(""));

        // Filename field
        let file_label_style = if self.add_phase == AddPhase::Filename {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(UNCHECKED_COLOR)
        };
        let file_value = if self.add_phase == AddPhase::Filename {
            format!("{}|", self.add_filename_buf)
        } else {
            self.add_filename_buf.clone()
        };
        lines.push(Line::from(vec![
            Span::styled("  Filename: ", file_label_style),
            Span::styled(file_value, Style::default().fg(Color::White)),
        ]));

        lines.push(Line::raw(""));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  Enter: Confirm | Esc: Cancel",
            Style::default().fg(Color::DarkGray),
        )));

        let widget = Paragraph::new(lines);
        frame.render_widget(widget, area);
    }
}
