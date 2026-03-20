use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::dashboard::FocusWidget;
use crate::views::filter_tree::SIDEBAR_WIDTH;
use crate::views::theme::{ACCENT, BAR_BG, FLASH_COLOR};
use crate::views::widget_selector::{VisibleWidgets, WidgetKind};

pub struct FrameLayout {
    pub titlebar: Rect,
    pub sidebar: Option<Rect>,
    pub table: Rect,
    pub right_widgets: Vec<(WidgetKind, Rect)>,
    pub bottom_widgets: Vec<(WidgetKind, Rect)>,
    pub statusbar: Rect,
}

pub fn build_frame(frame: &mut Frame, widgets: &VisibleWidgets) -> FrameLayout {
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
    let (sidebar, remaining) = if widgets.filters {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(SIDEBAR_WIDTH), Constraint::Min(10)])
            .split(content);
        (Some(cols[0]), cols[1])
    } else {
        (None, content)
    };

    let visible_right = widgets.visible_right_widgets();
    let total_right = visible_right.len();

    if total_right == 0 {
        return FrameLayout {
            titlebar,
            sidebar,
            table: remaining,
            right_widgets: Vec::new(),
            bottom_widgets: Vec::new(),
            statusbar,
        };
    }

    // Split into panel widgets (right side, max 4) and overflow (under table)
    let (panel_kinds, overflow_kinds) = if total_right <= 4 {
        (visible_right.clone(), Vec::new())
    } else {
        (visible_right[..4].to_vec(), visible_right[4..].to_vec())
    };

    let panel_count = panel_kinds.len();

    // Split remaining 50/50 into left column (table) and right column (widget panel)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(remaining);
    let (left_col, right_col) = (cols[0], cols[1]);

    // Split right column equally among panel widgets
    let right_constraints: Vec<Constraint> = (0..panel_count)
        .map(|_| Constraint::Ratio(1, panel_count as u32))
        .collect();
    let right_parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints(right_constraints)
        .split(right_col);

    let right_widgets: Vec<(WidgetKind, Rect)> = panel_kinds
        .into_iter()
        .zip(right_parts.iter().copied())
        .collect();

    // No overflow: table uses the whole left column
    if overflow_kinds.is_empty() {
        return FrameLayout {
            titlebar,
            sidebar,
            table: left_col,
            right_widgets,
            bottom_widgets: Vec::new(),
            statusbar,
        };
    }

    // Overflow: split left column into table (top) + bottom zone
    let bottom_height = right_parts.iter().map(|r| r.height).min().unwrap_or(3);
    let left_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(bottom_height)])
        .split(left_col);
    let table_rect = left_split[0];
    let bottom_zone = left_split[1];

    let bottom_widgets = if overflow_kinds.len() == 1 {
        vec![(overflow_kinds[0].clone(), bottom_zone)]
    } else {
        let halves = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(bottom_zone);
        overflow_kinds
            .into_iter()
            .zip(halves.iter().copied())
            .collect()
    };

    FrameLayout {
        titlebar,
        sidebar,
        table: table_rect,
        right_widgets,
        bottom_widgets,
        statusbar,
    }
}

pub fn render_titlebar(frame: &mut Frame, area: Rect, username: &str, flash: Option<&str>) {
    let bar_style = Style::default().bg(BAR_BG);

    // Fill background
    frame.render_widget(Block::default().style(bar_style), area);

    // Vertically center content on the middle row
    let mid_y = area.y + area.height / 2;
    let row = Rect {
        x: area.x,
        y: mid_y,
        width: area.width,
        height: 1,
    };

    // Left side: brand + optional flash
    let mut left_spans = vec![
        Span::styled("  sqwatch ", Style::default().fg(ACCENT).bg(BAR_BG).bold()),
        Span::styled(
            "- SLURM Queue Watcher ",
            Style::default().fg(Color::Rgb(170, 170, 190)).bg(BAR_BG),
        ),
    ];

    if let Some(msg) = flash {
        left_spans.push(Span::styled(
            " \u{2502} ",
            Style::default().fg(Color::DarkGray).bg(BAR_BG),
        ));
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

pub fn render_statusbar(
    frame: &mut Frame,
    area: Rect,
    counts: (usize, usize, usize),
    focus: &FocusWidget,
) {
    let bar_style = Style::default().bg(BAR_BG);

    // Fill background
    frame.render_widget(Block::default().style(bar_style), area);

    // Vertically center content on the middle row
    let mid_y = area.y + area.height / 2;
    let row = Rect {
        x: area.x,
        y: mid_y,
        width: area.width,
        height: 1,
    };

    // Global bindings (always shown)
    let mut bindings: Vec<(&str, &str)> = vec![
        ("Esc", "Quit"),
        ("Tab", "Focus"),
        ("\u{2191}\u{2193}", "Navigation"),
        ("Ctrl+W", "Widgets"),
        ("Ctrl+C", "Columns"),
    ];

    // Context-specific bindings per focused widget
    match focus {
        FocusWidget::Table => {}
        FocusWidget::Sidebar => {
            bindings.push(("Enter", "Edit/Toggle"));
            bindings.push(("Ctrl+S", "Save"));
        }
        FocusWidget::Script
        | FocusWidget::Stdout
        | FocusWidget::Stderr
        | FocusWidget::Custom(_) => {
            bindings.push(("PgUp/Dn", "Scroll"));
            bindings.push(("Ctrl+C", "Copy"));
        }
    }

    let mut spans: Vec<Span> = vec![Span::styled("  ", bar_style)];
    for (k, desc) in &bindings {
        spans.push(Span::styled(*k, Style::default().fg(ACCENT).bg(BAR_BG)));
        spans.push(Span::styled(
            format!(" {} ", desc),
            Style::default().fg(Color::Rgb(170, 170, 190)).bg(BAR_BG),
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
