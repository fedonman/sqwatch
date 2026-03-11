pub mod input;
pub mod live_file;

pub fn current_user() -> String {
    std::env::var("USER").unwrap_or_else(|_| "unknown".to_string())
}

pub fn _shorten(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        text.to_string()
    } else {
        format!("{}...", &text[..limit - 3])
    }
}

pub fn _human_memory(megabytes: u64) -> String {
    if megabytes < 1024 {
        format!("{}M", megabytes)
    } else {
        format!("{:.1}G", megabytes as f64 / 1024.0)
    }
}

pub fn _human_duration(total_secs: u64) -> String {
    let d = total_secs / 86400;
    let h = (total_secs % 86400) / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;

    if d > 0 {
        format!("{}d {:02}:{:02}:{:02}", d, h, m, s)
    } else {
        format!("{:02}:{:02}:{:02}", h, m, s)
    }
}
