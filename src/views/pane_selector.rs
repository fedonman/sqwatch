use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

const POPUP_BG: Color = Color::Rgb(15, 15, 30);
const ACCENT: Color = Color::Magenta;
const CHECKED_COLOR: Color = Color::Rgb(80, 200, 255);
const UNCHECKED_COLOR: Color = Color::Rgb(100, 100, 100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneKind {
    Filters,
    Script,
    Stdout,
    Stderr,
}

impl PaneKind {
    pub fn label(&self) -> &'static str {
        match self {
            PaneKind::Filters => "Filters",
            PaneKind::Script => "Execution Script",
            PaneKind::Stdout => "stdout",
            PaneKind::Stderr => "stderr",
        }
    }
}

const PANE_ORDER: [PaneKind; 4] = [
    PaneKind::Filters,
    PaneKind::Script,
    PaneKind::Stdout,
    PaneKind::Stderr,
];

#[derive(Debug, Clone, Copy)]
pub struct VisiblePanes {
    pub filters: bool,
    pub script: bool,
    pub stdout: bool,
    pub stderr: bool,
}

impl Default for VisiblePanes {
    fn default() -> Self {
        Self {
            filters: false,
            script: true,
            stdout: true,
            stderr: false,
        }
    }
}

impl VisiblePanes {
    pub fn is_visible(&self, kind: PaneKind) -> bool {
        match kind {
            PaneKind::Filters => self.filters,
            PaneKind::Script => self.script,
            PaneKind::Stdout => self.stdout,
            PaneKind::Stderr => self.stderr,
        }
    }

    pub fn toggle(&mut self, kind: PaneKind) {
        match kind {
            PaneKind::Filters => self.filters = !self.filters,
            PaneKind::Script => self.script = !self.script,
            PaneKind::Stdout => self.stdout = !self.stdout,
            PaneKind::Stderr => self.stderr = !self.stderr,
        }
    }

    pub fn right_pane_count(&self) -> usize {
        [self.script, self.stdout, self.stderr]
            .iter()
            .filter(|&&v| v)
            .count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneSelectorAction {
    Noop,
    Dismiss,
    Changed,
}

pub struct PaneSelector {
    pub visible: bool,
    cursor: usize,
}

impl PaneSelector {
    pub fn new() -> Self {
        Self {
            visible: false,
            cursor: 0,
        }
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        panes: &mut VisiblePanes,
    ) -> PaneSelectorAction {
        match key.code {
            KeyCode::Esc => PaneSelectorAction::Dismiss,
            KeyCode::Up => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                PaneSelectorAction::Noop
            }
            KeyCode::Down => {
                if self.cursor < PANE_ORDER.len() - 1 {
                    self.cursor += 1;
                }
                PaneSelectorAction::Noop
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let kind = PANE_ORDER[self.cursor];
                panes.toggle(kind);
                PaneSelectorAction::Changed
            }
            _ => PaneSelectorAction::Noop,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, panes: &VisiblePanes) {
        frame.render_widget(Clear, area);

        let outer = Block::default()
            .title(Line::from(" \u{25c6} Pane Layout \u{25c6} ").centered())
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

        for (i, kind) in PANE_ORDER.iter().enumerate() {
            let checked = panes.is_visible(*kind);
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

        let hint = " \u{2191}\u{2193}: Navigate | Enter: Toggle | Esc: Close";
        lines.push(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )));

        let widget = Paragraph::new(lines);
        frame.render_widget(widget, inner);
    }
}
