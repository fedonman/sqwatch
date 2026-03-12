use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

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
    filters: &str,
    username: &str,
    flash: Option<&str>,
) {
    let brand_width = "sqwatch - SLURM Queue Watcher".len() as u16 + 2;
    let user_label = "User: ";
    let user_width = user_label.len() as u16 + username.len() as u16 + 2; // +2 for borders

    let filter_label = "Filters: ";
    let filter_width = filter_label.len() as u16 + filters.len() as u16 + 2; // +2 for borders

    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(brand_width),
            Constraint::Length(user_width),
            Constraint::Length(filter_width),
            Constraint::Min(0),
        ])
        .split(area);

    let brand = Paragraph::new(Text::from(vec![Line::from(vec![
        Span::styled("sqwatch", Style::default().fg(Color::Cyan).bold()),
        Span::raw(" - "),
        Span::styled("SLURM Queue Watcher", Style::default().fg(Color::White)),
    ])]))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(brand, sections[0]);

    let user_box = Paragraph::new(Line::from(vec![
        Span::styled(user_label, Style::default().fg(Color::White)),
        Span::styled(username, Style::default().fg(Color::Yellow)),
    ]))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(user_box, sections[1]);

    let filter_bar = Paragraph::new(Line::from(vec![
        Span::styled("Filters: ", Style::default().fg(Color::White)),
        Span::raw(filters),
    ]))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(filter_bar, sections[2]);

    let flash_content = flash.unwrap_or("");
    let flash_bar = Paragraph::new(Line::from(vec![
        Span::styled(flash_content, Style::default().fg(Color::Yellow)),
    ]))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(flash_bar, sections[3]);
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
        ("x", "Cancel"),
    ];

    let key_spans: Vec<Span> = bindings
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

    let stat_spans = vec![
        Span::styled("Job Stat: ", accent),
        Span::styled(
            format!("P[ {} ] ", counts.0),
            Style::default().fg(Color::Yellow),
        ),
        Span::styled(
            format!("R[ {} ] ", counts.1),
            Style::default().fg(Color::Green),
        ),
        Span::styled(
            format!("Other[ {} ]", counts.2),
            Style::default().fg(Color::Blue),
        ),
    ];

    // Calculate stat width for right-aligned box
    let stat_text_len: usize = stat_spans.iter().map(|s| s.width()).sum();
    let stat_width = stat_text_len as u16 + 2; // +2 for borders

    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(stat_width)])
        .split(area);

    let keys_bar = Paragraph::new(Line::from(key_spans))
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(keys_bar, sections[0]);

    let stat_bar = Paragraph::new(Line::from(stat_spans))
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(stat_bar, sections[1]);
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
