use ratatui::style::Color;

pub const ACCENT: Color = Color::Rgb(200, 120, 255);
pub const BAR_BG: Color = Color::Rgb(22, 22, 40);
pub const POPUP_BG: Color = Color::Rgb(15, 15, 30);
pub const DIM_BORDER: Color = Color::Rgb(60, 60, 85);
pub const FLASH_COLOR: Color = Color::Rgb(255, 200, 80);
pub const CHECKED_COLOR: Color = Color::Rgb(80, 200, 255);
pub const UNCHECKED_COLOR: Color = Color::Rgb(140, 140, 140);

// Per-widget accent colors for focused borders
pub const ACCENT_SCRIPT: Color = Color::Rgb(80, 200, 220);
pub const ACCENT_STDOUT: Color = Color::Rgb(80, 210, 150);
pub const ACCENT_STDERR: Color = Color::Rgb(230, 100, 100);
pub const ACCENT_CUSTOM: Color = Color::Rgb(180, 130, 255);
pub const ACCENT_SIDEBAR: Color = Color::Rgb(255, 180, 80);

// Table row highlighting
pub const ROW_HIGHLIGHT_BG: Color = Color::Rgb(40, 30, 65);
