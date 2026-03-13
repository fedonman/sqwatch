use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::dashboard::FocusPanel;
use crate::views::filter_tree::SIDEBAR_WIDTH;

const BAR_BG: Color = Color::Rgb(30, 30, 50);
const ACCENT: Color = Color::Magenta;
const FLASH_COLOR: Color = Color::Rgb(255, 200, 80);

pub struct FrameLayout {
    pub titlebar: Rect,
    pub sidebar: Option<Rect>,
    pub table: Rect,
    pub script: Rect,
    pub output: Rect,
    pub statusbar: Rect,
}

pub fn build_frame(frame: &mut Frame, sidebar_open: bool) -> FrameLayout {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let titlebar = rows[0];
    let statusbar = rows[2];
    let content = rows[1];

    // Split content into columns: [sidebar?] | table | right-panels
    let (sidebar, table_area, right_area) = if sidebar_open {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(SIDEBAR_WIDTH),
                Constraint::Percentage(50),
                Constraint::Min(25),
            ])
            .split(content);
        (Some(cols[0]), cols[1], cols[2])
    } else {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Min(25)])
            .split(content);
        (None, cols[0], cols[1])
    };

    // Split right area into script (top) and output (bottom)
    let right_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(right_area);

    FrameLayout {
        titlebar,
        sidebar,
        table: table_area,
        script: right_split[0],
        output: right_split[1],
        statusbar,
    }
}

pub fn render_titlebar(
    frame: &mut Frame,
    area: Rect,
    filters: &str,
    username: &str,
    flash: Option<&str>,
) {
    let bar_style = Style::default().bg(BAR_BG);

    // Fill background
    frame.render_widget(Block::default().style(bar_style), area);

    // Vertically center content on the middle row
    let mid_y = area.y + area.height / 2;
    let row = Rect { x: area.x, y: mid_y, width: area.width, height: 1 };

    // Left side: brand + separator + filters
    let mut left_spans = vec![
        Span::styled("  sqwatch ", Style::default().fg(ACCENT).bg(BAR_BG).bold()),
        Span::styled("- SLURM Queue Watcher ", Style::default().fg(Color::Rgb(140, 140, 140)).bg(BAR_BG)),
        Span::styled(" \u{2502} ", Style::default().fg(Color::DarkGray).bg(BAR_BG)),
        Span::styled(
            filters,
            Style::default().fg(Color::Rgb(180, 180, 180)).bg(BAR_BG),
        ),
    ];

    // Flash text after filters
    if let Some(msg) = flash {
        left_spans.push(Span::styled(" \u{2502} ", Style::default().fg(Color::DarkGray).bg(BAR_BG)));
        left_spans.push(Span::styled(
            msg,
            Style::default().fg(FLASH_COLOR).bg(BAR_BG),
        ));
    }

    let widget = Paragraph::new(Line::from(left_spans)).style(bar_style);
    frame.render_widget(widget, row);

    // Right-aligned user info
    let user_text = format!("  {}  ", username);
    let user_width = user_text.len() as u16;
    if row.width > user_width + 2 {
        let user_area = Rect {
            x: row.x + row.width - user_width,
            y: mid_y,
            width: user_width,
            height: 1,
        };
        let user_widget = Paragraph::new(Line::from(vec![Span::styled(
            user_text,
            Style::default().fg(Color::Green).bg(BAR_BG),
        )]))
        .style(bar_style);
        frame.render_widget(user_widget, user_area);
    }
}

pub fn render_statusbar(frame: &mut Frame, area: Rect, counts: (usize, usize, usize), focus: FocusPanel) {
    let bar_style = Style::default().bg(BAR_BG);

    // Fill background
    frame.render_widget(Block::default().style(bar_style), area);

    // Vertically center content on the middle row
    let mid_y = area.y + area.height / 2;
    let row = Rect { x: area.x, y: mid_y, width: area.width, height: 1 };

    // Global bindings (always shown)
    let mut bindings: Vec<(&str, &str)> = vec![
        ("Esc", "Quit"),
        ("\u{2191}\u{2193}", "Nav"),
        ("Tab", "Focus"),
        ("f", "Sidebar"),
    ];

    // Context-specific bindings per focused pane
    match focus {
        FocusPanel::Table => {
            bindings.push(("c", "Columns"));
            bindings.push(("Space", "Mark"));
            bindings.push(("a", "Mark All"));
            bindings.push(("x", "Cancel"));
        }
        FocusPanel::Sidebar => {
            bindings.push(("Enter", "Edit/Toggle"));
            bindings.push(("r", "Reset"));
            bindings.push(("Ctrl+S", "Save"));
        }
        FocusPanel::Output => {
            bindings.push(("o", "Toggle Stream"));
            bindings.push(("PgUp/Dn", "Scroll"));
        }
        FocusPanel::Script => {}
    }

    let mut spans: Vec<Span> = vec![Span::styled("  ", bar_style)];
    for (k, desc) in &bindings {
        spans.push(Span::styled(
            *k,
            Style::default().fg(ACCENT).bg(BAR_BG),
        ));
        spans.push(Span::styled(
            format!(" {} ", desc),
            Style::default().fg(Color::Rgb(140, 140, 140)).bg(BAR_BG),
        ));
    }

    let widget = Paragraph::new(Line::from(spans)).style(bar_style);
    frame.render_widget(widget, row);

    // Right-aligned stats
    let stat_text = format!("  P:{}  R:{}  O:{}  ", counts.0, counts.1, counts.2);
    let stat_width = stat_text.len() as u16;
    if row.width > stat_width + 2 {
        let stat_area = Rect {
            x: row.x + row.width - stat_width,
            y: mid_y,
            width: stat_width,
            height: 1,
        };
        let stat_spans = vec![
            Span::styled("  ", Style::default().bg(BAR_BG)),
            Span::styled(
                format!("P:{}", counts.0),
                Style::default().fg(Color::Rgb(255, 170, 50)).bg(BAR_BG),
            ),
            Span::styled("  ", Style::default().bg(BAR_BG)),
            Span::styled(
                format!("R:{}", counts.1),
                Style::default().fg(Color::Rgb(50, 210, 170)).bg(BAR_BG),
            ),
            Span::styled("  ", Style::default().bg(BAR_BG)),
            Span::styled(
                format!("O:{}", counts.2),
                Style::default().fg(Color::Rgb(120, 140, 180)).bg(BAR_BG),
            ),
            Span::styled("  ", Style::default().bg(BAR_BG)),
        ];
        let stat_widget = Paragraph::new(Line::from(stat_spans)).style(bar_style);
        frame.render_widget(stat_widget, stat_area);
    }
}

pub fn popup_rect(parent: Rect, pct_w: u16, pct_h: u16) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_h) / 2),
            Constraint::Percentage(pct_h),
            Constraint::Percentage((100 - pct_h) / 2),
        ])
        .split(parent);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_w) / 2),
            Constraint::Percentage(pct_w),
            Constraint::Percentage((100 - pct_w) / 2),
        ])
        .split(vert[1])[1]
}
