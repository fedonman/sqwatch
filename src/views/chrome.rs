use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

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
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
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

    let mut spans = vec![
        Span::styled(" sqwatch ", Style::default().fg(ACCENT).bg(BAR_BG).bold()),
        Span::styled(" \u{2502} ", Style::default().fg(Color::DarkGray).bg(BAR_BG)),
    ];

    if let Some(msg) = flash {
        spans.push(Span::styled(
            msg,
            Style::default().fg(FLASH_COLOR).bg(BAR_BG),
        ));
    } else {
        spans.push(Span::styled(
            filters,
            Style::default().fg(Color::Rgb(180, 180, 180)).bg(BAR_BG),
        ));
    }

    let widget = Paragraph::new(Line::from(spans))
        .style(bar_style)
        .block(Block::default().borders(Borders::NONE));

    frame.render_widget(widget, area);

    // Right-aligned user info
    let user_text = format!("{} ", username);
    let user_width = user_text.len() as u16;
    if area.width > user_width + 2 {
        let user_area = Rect {
            x: area.x + area.width - user_width,
            y: area.y,
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

pub fn render_statusbar(frame: &mut Frame, area: Rect, counts: (usize, usize, usize)) {
    let bar_style = Style::default().bg(BAR_BG);

    let bindings = [
        ("Esc", "Quit"),
        ("\u{2191}\u{2193}", "Nav"),
        ("Tab", "Focus"),
        ("f", "Sidebar"),
        ("c", "Columns"),
    ];

    let mut spans: Vec<Span> = vec![Span::styled(" ", bar_style)];
    for (k, desc) in bindings {
        spans.push(Span::styled(
            k,
            Style::default().fg(ACCENT).bg(BAR_BG),
        ));
        spans.push(Span::styled(
            format!(" {} ", desc),
            Style::default().fg(Color::Rgb(140, 140, 140)).bg(BAR_BG),
        ));
    }

    let widget = Paragraph::new(Line::from(spans))
        .style(bar_style)
        .block(Block::default().borders(Borders::NONE));

    frame.render_widget(widget, area);

    // Right-aligned stats
    let stat_width = format!("P:{} R:{} O:{} ", counts.0, counts.1, counts.2).len() as u16;
    if area.width > stat_width + 2 {
        let stat_area = Rect {
            x: area.x + area.width - stat_width,
            y: area.y,
            width: stat_width,
            height: 1,
        };
        let stat_spans = vec![
            Span::styled(
                format!("P:{} ", counts.0),
                Style::default().fg(Color::Rgb(255, 170, 50)).bg(BAR_BG),
            ),
            Span::styled(
                format!("R:{} ", counts.1),
                Style::default().fg(Color::Rgb(50, 210, 170)).bg(BAR_BG),
            ),
            Span::styled(
                format!("O:{} ", counts.2),
                Style::default().fg(Color::Rgb(120, 140, 180)).bg(BAR_BG),
            ),
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
