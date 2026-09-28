use ratatui::widgets::{Paragraph, Wrap};

/// Where each line of a log starts once it is word-wrapped to a pane's width.
///
/// A wrapped `Paragraph` has to wrap every line above its scroll offset before
/// it can draw, and measuring it wraps the rest, so a pane that draws the whole
/// log costs time in proportion to the log on every frame. The index wraps each
/// line once, and [`WrapIndex::window`] then picks out just the lines on
/// screen. Tell it about every change of text with [`WrapIndex::text_changed`];
/// a log that has only grown keeps the lines it already wrapped.
#[derive(Debug, Default)]
pub struct WrapIndex {
    width: u16,
    /// How many bytes at the start of the text the index covers.
    indexed: usize,
    /// For each line of the text, its byte offset and the first wrapped row it
    /// is drawn on.
    lines: Vec<(usize, usize)>,
    rows: usize,
}

impl WrapIndex {
    /// Note that the text is changing from `old` to `new`.
    pub fn text_changed(&mut self, old: &str, new: &str) {
        let unchanged = if new.starts_with(old) { old.len() } else { 0 };
        self.indexed = self.indexed.min(unchanged);
    }

    /// The number of rows `text` takes when wrapped to `width`.
    pub fn rows(&mut self, text: &str, width: u16) -> usize {
        if width != self.width || self.indexed > text.len() {
            self.width = width;
            self.indexed = 0;
        }
        if self.indexed != text.len() {
            self.extend(text);
        }
        self.rows
    }

    /// The part of `text` that holds wrapped rows `first..first + height`,
    /// and how many of its own rows to scroll past to reach `first`. Call
    /// [`WrapIndex::rows`] with the same text and width first.
    pub fn window<'a>(&self, text: &'a str, first: usize, height: usize) -> (&'a str, u16) {
        let start = self
            .lines
            .partition_point(|&(_, row)| row <= first)
            .saturating_sub(1);
        let end = self.lines.partition_point(|&(_, row)| row < first + height);

        let Some(&(from, start_row)) = self.lines.get(start) else {
            return ("", 0);
        };
        let to = self.lines.get(end).map_or(text.len(), |&(byte, _)| byte);
        let skip = u16::try_from(first.saturating_sub(start_row)).unwrap_or(u16::MAX);
        (text.get(from..to).unwrap_or(""), skip)
    }

    /// Wrap the lines past the indexed part of `text`. The last indexed line
    /// is wrapped again, since text appended to it may have lengthened it.
    fn extend(&mut self, text: &str) {
        let mut keep = self
            .lines
            .partition_point(|&(byte, _)| byte < self.indexed)
            .saturating_sub(1);
        // An index that no longer lines up with the text starts over rather
        // than slicing it in the wrong place.
        if self
            .lines
            .get(keep)
            .is_some_and(|&(byte, _)| !text.is_char_boundary(byte))
        {
            keep = 0;
        }
        let (mut byte, mut rows) = self.lines.get(keep).copied().unwrap_or_default();
        self.lines.truncate(keep);

        // `Text` splits on '\n' the same way, so each piece wraps exactly as
        // it would as part of the whole text.
        for line in text[byte..].split_inclusive('\n') {
            self.lines.push((byte, rows));
            rows += Paragraph::new(line)
                .wrap(Wrap { trim: false })
                .line_count(self.width);
            byte += line.len();
        }
        self.rows = rows;
        self.indexed = text.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

    const LOG: &str = "short\n\
        a line long enough that it has to wrap over several rows of a narrow pane\n\
        \n\
        windows line ending\r\n\
        averyveryverylongwordwithnospacesthatcannotbreakanywhere\n\
        no newline at the end";

    fn whole_rows(text: &str, width: u16) -> usize {
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .line_count(width)
    }

    fn draw(para: Paragraph, width: u16, height: u16) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        para.wrap(Wrap { trim: false }).render(area, &mut buf);
        buf
    }

    fn assert_window_matches_whole_text(index: &WrapIndex, text: &str, width: u16) {
        let height = 4;
        for first in 0..whole_rows(text, width) {
            let (visible, skip) = index.window(text, first, height as usize);
            assert_eq!(
                draw(Paragraph::new(visible).scroll((skip, 0)), width, height),
                draw(
                    Paragraph::new(text).scroll((first as u16, 0)),
                    width,
                    height
                ),
                "first row {}",
                first
            );
        }
    }

    #[test]
    fn rows_match_wrapping_the_whole_text() {
        for width in [1, 7, 20, 80] {
            let rows = WrapIndex::default().rows(LOG, width);
            assert_eq!(rows, whole_rows(LOG, width), "width {}", width);
        }
    }

    #[test]
    fn the_window_draws_the_same_rows_as_scrolling_the_whole_text() {
        let mut index = WrapIndex::default();
        index.rows(LOG, 20);
        assert_window_matches_whole_text(&index, LOG, 20);
    }

    #[test]
    fn a_growing_log_is_indexed_as_if_read_whole() {
        let mut index = WrapIndex::default();
        let mut old = "";
        // Cuts inside a line as well as after one, so appends lengthen the
        // last line as well as adding new ones.
        for cut in [3, 6, 40, 80, 81, 120, LOG.len()] {
            let new = &LOG[..cut];
            index.text_changed(old, new);
            assert_eq!(index.rows(new, 20), whole_rows(new, 20), "cut at {}", cut);
            old = new;
        }
        assert_window_matches_whole_text(&index, LOG, 20);
    }

    #[test]
    fn replaced_text_is_indexed_afresh() {
        let mut index = WrapIndex::default();
        assert_eq!(index.rows("aaaa aaaa", 4), whole_rows("aaaa aaaa", 4));

        index.text_changed("aaaa aaaa", "aaaaaaaaa");
        assert_eq!(index.rows("aaaaaaaaa", 4), whole_rows("aaaaaaaaa", 4));
    }

    #[test]
    fn a_new_width_rebuilds() {
        let mut index = WrapIndex::default();
        index.rows(LOG, 20);
        assert_eq!(index.rows(LOG, 7), whole_rows(LOG, 7));
    }
}
