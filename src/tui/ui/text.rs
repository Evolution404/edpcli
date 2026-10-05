//! Cell-aware wrapping shared by scrollable modal views and their navigation metrics.
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub(crate) fn wrap_lines(lines: Vec<Line<'_>>, width: u16) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let mut result = Vec::new();
    for line in lines {
        let mut row = Vec::new();
        let mut cells = 0;
        for span in line.spans {
            for grapheme in span.content.graphemes(true) {
                let size = UnicodeWidthStr::width(grapheme);
                if cells + size > usize::from(width) && !row.is_empty() {
                    result.push(Line::from(std::mem::take(&mut row)).style(line.style));
                    cells = 0;
                }
                if size <= usize::from(width) {
                    row.push(Span::styled(grapheme.to_owned(), span.style));
                    cells += size;
                }
            }
        }
        result.push(Line::from(row).style(line.style));
    }
    result
}
