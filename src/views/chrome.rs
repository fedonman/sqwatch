use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::dashboard::FocusPanel;
use crate::views::filter_tree::SIDEBAR_WIDTH;
use crate::views::pane_selector::VisiblePanes;

const BAR_BG: Color = Color::Rgb(30, 30, 50);
const ACCENT: Color = Color::Magenta;
const FLASH_COLOR: Color = Color::Rgb(255, 200, 80);

pub struct FrameLayout {
    pub titlebar: Rect,
    pub sidebar: Option<Rect>,
    pub table: Rect,
    pub script: Option<Rect>,
    pub stdout: Option<Rect>,
    pub stderr: Option<Rect>,
    pub statusbar: Rect,
}

pub fn build_frame(frame: &mut Frame, panes: &VisiblePanes) -> FrameLayout {
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

    // Split off sidebar if visible
    let (sidebar, remaining) = if panes.filters {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(SIDEBAR_WIDTH),
                Constraint::Min(10),
            ])
            .split(content);
        (Some(cols[0]), cols[1])
    } else {
        (None, content)
    };

    // Split remaining into table and right area (50/50) if any right panes exist
    let right_count = panes.right_pane_count();
    let (table_area, right_area) = if right_count > 0 {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(remaining);
        (cols[0], Some(cols[1]))
    } else {
        (remaining, None)
    };

    // Split right area vertically among visible right panes
    let (mut script_rect, mut stdout_rect, mut stderr_rect) = (None, None, None);

    if let Some(right) = right_area {
        let constraints: Vec<Constraint> = (0..right_count)
            .map(|_| Constraint::Ratio(1, right_count as u32))
            .collect();

        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(right);

        let mut idx = 0;
        if panes.script {
            script_rect = Some(parts[idx]);
            idx += 1;
        }
        if panes.stdout {
            stdout_rect = Some(parts[idx]);
            idx += 1;
        }
        if panes.stderr {
            stderr_rect = Some(parts[idx]);
        }
    }

    FrameLayout {
        titlebar,
        sidebar,
        table: table_area,
        script: script_rect,
        stdout: stdout_rect,
        stderr: stderr_rect,
        statusbar,
    }
}

pub fn render_titlebar(
    frame: &mut Frame,
    area: Rect,
    username: &str,
    flash: Option<&str>,
) {
    let bar_style = Style::default().bg(BAR_BG);

    // Fill background
    frame.render_widget(Block::default().style(bar_style), area);

    // Vertically center content on the middle row
    let mid_y = area.y + area.height / 2;
    let row = Rect { x: area.x, y: mid_y, width: area.width, height: 1 };

    // Left side: brand + optional flash
    let mut left_spans = vec![
        Span::styled("  sqwatch ", Style::default().fg(ACCENT).bg(BAR_BG).bold()),
        Span::styled("- SLURM Queue Watcher ", Style::default().fg(Color::Rgb(140, 140, 140)).bg(BAR_BG)),
    ];

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
        ("w", "Layout"),
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
        FocusPanel::Stdout | FocusPanel::Stderr => {
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
