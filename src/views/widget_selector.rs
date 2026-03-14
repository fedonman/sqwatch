use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use super::theme::{ACCENT, CHECKED_COLOR, POPUP_BG, UNCHECKED_COLOR};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetKind {
    Filters,
    Script,
    Stdout,
    Stderr,
}

impl WidgetKind {
    pub fn label(&self) -> &'static str {
        match self {
            WidgetKind::Filters => "Filters",
            WidgetKind::Script => "Execution Script",
            WidgetKind::Stdout => "stdout",
            WidgetKind::Stderr => "stderr",
        }
    }
}

const WIDGET_ORDER: [WidgetKind; 4] = [
    WidgetKind::Filters,
    WidgetKind::Script,
    WidgetKind::Stdout,
    WidgetKind::Stderr,
];

#[derive(Debug, Clone, Copy)]
pub struct VisibleWidgets {
    pub filters: bool,
    pub script: bool,
    pub stdout: bool,
    pub stderr: bool,
}

impl Default for VisibleWidgets {
    fn default() -> Self {
        Self {
            filters: false,
            script: true,
            stdout: true,
            stderr: false,
        }
    }
}

impl VisibleWidgets {
    pub fn is_visible(&self, kind: WidgetKind) -> bool {
        match kind {
            WidgetKind::Filters => self.filters,
            WidgetKind::Script => self.script,
            WidgetKind::Stdout => self.stdout,
            WidgetKind::Stderr => self.stderr,
        }
    }

    pub fn toggle(&mut self, kind: WidgetKind) {
        match kind {
            WidgetKind::Filters => self.filters = !self.filters,
            WidgetKind::Script => self.script = !self.script,
            WidgetKind::Stdout => self.stdout = !self.stdout,
            WidgetKind::Stderr => self.stderr = !self.stderr,
        }
    }

    pub fn right_widget_count(&self) -> usize {
        [self.script, self.stdout, self.stderr]
            .iter()
            .filter(|&&v| v)
            .count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetSelectorAction {
    Noop,
    Dismiss,
    Changed,
    Save,
}

pub struct WidgetSelector {
    pub visible: bool,
    cursor: usize,
}

impl WidgetSelector {
    pub fn new() -> Self {
        Self {
            visible: false,
            cursor: 0,
        }
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        widgets: &mut VisibleWidgets,
    ) -> WidgetSelectorAction {
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
                if self.cursor < WIDGET_ORDER.len() - 1 {
                    self.cursor += 1;
                }
                WidgetSelectorAction::Noop
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let kind = WIDGET_ORDER[self.cursor];
                widgets.toggle(kind);
                WidgetSelectorAction::Changed
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

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::raw(""));

        for (i, kind) in WIDGET_ORDER.iter().enumerate() {
            let checked = widgets.is_visible(*kind);
            let is_cursor = i == self.cursor;
            let mark = if checked { "\u{25c6}" } else { "\u{25c7}" };
            let color = if checked {
                CHECKED_COLOR
            } else {
                UNCHECKED_COLOR
            };
            let text = format!("  {} {}", mark, kind.label());

            let mut style = Style::default().fg(color);
            if is_cursor {
                style = style.fg(Color::White).add_modifier(Modifier::BOLD);
            }

            lines.push(Line::from(Span::styled(text, style)));
        }

        lines.push(Line::raw(""));

        let hint = " \u{2191}\u{2193}: Navigate | Enter: Toggle | Ctrl+S: Save | Esc: Close";
        lines.push(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )));

        let widget = Paragraph::new(lines);
        frame.render_widget(widget, inner);
    }
}
