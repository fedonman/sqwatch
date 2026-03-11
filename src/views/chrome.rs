use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::time::Duration;

pub fn build_frame(frame: &mut Frame) -> Vec<Rect> {
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(frame.area());

    vec![regions[0], regions[1], regions[2]]
}

pub fn render_titlebar(
    frame: &mut Frame,
    area: Rect,
    info: &str,
    age: Duration,
    interval: u64,
) {
    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(area);

    let brand = Paragraph::new(Text::from(vec![Line::from(vec![
        Span::styled("sqwatch", Style::default().fg(Color::Cyan).bold()),
        Span::raw(" - "),
        Span::styled("SLURM Queue Watcher", Style::default().fg(Color::White)),
    ])]))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(brand, halves[0]);

    let detail = format!(
        "{} | Refresh: {}s ago (auto: {}s)",
        info,
        age.as_secs(),
        interval
    );

    let status_bar = Paragraph::new(detail)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default());

    frame.render_widget(status_bar, halves[1]);
}

pub fn render_statusbar(frame: &mut Frame, area: Rect, counts: (usize, usize, usize)) {
    let accent = Style::default().fg(Color::Cyan);
    let bindings = [
        ("Esc", "Quit"),
        ("\u{2191}/\u{2193}", "Navigate"),
        ("Space", "Select"),
        ("Enter", "Script"),
        ("f", "Filter"),
        ("c", "Columns"),
        ("v", "Log"),
        ("a", "SelectAll"),
        ("r", "Refresh"),
        ("x", "Cancel"),
    ];

    let mut spans: Vec<Span> = bindings
        .iter()
        .flat_map(|(k, desc)| {
            vec![
                Span::styled(*k, accent),
                Span::raw(": "),
                Span::raw(*desc),
                Span::raw(" "),
            ]
        })
        .collect();

    spans.push(Span::styled("Job Stat: ", accent));
    spans.push(Span::styled(
        format!("P[ {} ] ", counts.0),
        Style::default().fg(Color::Yellow),
    ));
    spans.push(Span::styled(
        format!("R[ {} ] ", counts.1),
        Style::default().fg(Color::Green),
    ));
    spans.push(Span::styled(
        format!("Other[ {} ]", counts.2),
        Style::default().fg(Color::Blue),
    ));

    let bar = Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::ALL));
    frame.render_widget(bar, area);
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
