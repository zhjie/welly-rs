use crate::backend::snapshot::TerminalSnapshot;
use eframe::egui;

pub use crate::backend::input::GridPoint;

// Terminal grid dimensions are duplicated from main.rs constants to keep
// this module from depending on main. They are an invariant of the Welly
// experience, not a tunable.
const TERMINAL_COLS: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub start: GridPoint,
    pub end: GridPoint,
}

impl Selection {
    pub fn new(point: GridPoint) -> Self {
        Self {
            start: point,
            end: point,
        }
    }

    pub fn normalized(self) -> (GridPoint, GridPoint) {
        if grid_index(self.start) <= grid_index(self.end) {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }
}

pub fn grid_index(point: GridPoint) -> usize {
    point.row * TERMINAL_COLS + point.col
}

pub fn pos_to_grid_point(
    pos: egui::Pos2,
    rect: egui::Rect,
    cell_width: f32,
    cell_height: f32,
    rows: usize,
    cols: usize,
) -> Option<GridPoint> {
    if !rect.contains(pos) {
        return None;
    }

    let col = ((pos.x - rect.min.x) / cell_width).floor() as usize;
    let row = ((pos.y - rect.min.y) / cell_height).floor() as usize;
    Some(GridPoint {
        row: row.min(rows.saturating_sub(1)),
        col: col.min(cols.saturating_sub(1)),
    })
}

pub fn selected_text(snap: &TerminalSnapshot<'_>, selection: Selection) -> String {
    let (start, end) = selection.normalized();
    let mut lines = Vec::new();

    for row in start.row..=end.row {
        let start_col = if row == start.row { start.col } else { 0 };
        let end_col = if row == end.row {
            end.col
        } else {
            snap.cols.saturating_sub(1)
        };

        let mut line = String::new();
        for col in start_col..=end_col.min(snap.cols.saturating_sub(1)) {
            let cell = &snap.rows[row][col];
            if cell.width == 0 {
                continue;
            }
            line.push(cell.ch);
        }
        lines.push(line.trim_end().to_owned());
    }

    lines.join("\n")
}

pub fn terminal_screen_text(snap: &TerminalSnapshot<'_>) -> String {
    let selection = Selection {
        start: GridPoint { row: 0, col: 0 },
        end: GridPoint {
            row: snap.row_count.saturating_sub(1),
            col: snap.cols.saturating_sub(1),
        },
    };
    selected_text(snap, selection)
}

pub fn url_at_grid_point(snap: &TerminalSnapshot<'_>, point: GridPoint) -> Option<String> {
    if point.row >= snap.row_count || point.col >= snap.cols {
        return None;
    }

    let row = &snap.rows[point.row];
    let mut text = String::new();
    let mut char_cells = Vec::new();
    for (col, cell) in row.iter().enumerate() {
        if cell.width == 0 {
            continue;
        }

        let start_byte = text.len();
        text.push(cell.ch);
        char_cells.push(VisibleCharCell {
            start_byte,
            end_byte: text.len(),
            start_col: col,
            end_col: col + cell.width.max(1) as usize - 1,
        });
    }

    for start in http_url_starts(&text) {
        let mut end = text[start..]
            .char_indices()
            .find_map(|(offset, ch)| ch.is_whitespace().then_some(start + offset))
            .unwrap_or(text.len());
        while end > start {
            let Some(ch) = text[..end].chars().next_back() else {
                break;
            };
            if !is_trailing_url_punctuation(ch) {
                break;
            }
            end -= ch.len_utf8();
        }

        if start == end {
            continue;
        }

        let Some(start_cell) = char_cells.iter().find(|cell| cell.start_byte == start) else {
            continue;
        };
        let Some(end_cell) = char_cells
            .iter()
            .rev()
            .find(|cell| cell.end_byte <= end && cell.start_byte >= start)
        else {
            continue;
        };

        if (start_cell.start_col..=end_cell.end_col).contains(&point.col) {
            return Some(text[start..end].to_owned());
        }
    }

    None
}

#[derive(Clone, Copy)]
struct VisibleCharCell {
    start_byte: usize,
    end_byte: usize,
    start_col: usize,
    end_col: usize,
}

fn http_url_starts(text: &str) -> Vec<usize> {
    let mut starts: Vec<usize> = text
        .match_indices("https://")
        .map(|(index, _)| index)
        .collect();
    starts.extend(text.match_indices("http://").map(|(index, _)| index));
    starts.sort_unstable();
    starts.dedup();
    starts
}

fn is_trailing_url_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '.' | ','
            | ';'
            | ':'
            | '!'
            | '?'
            | ')'
            | ']'
            | '}'
            | '>'
            | '"'
            | '\''
            | '。'
            | '，'
            | '；'
            | '：'
            | '！'
            | '？'
            | '）'
            | '】'
            | '》'
            | '\u{201d}'
            | '\u{2019}'
    )
}

pub fn normalize_selected_url_for_open(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return None;
    }

    let trimmed = trim_url_trailing_punctuation(trimmed);
    if is_scheme_url(trimmed) {
        return Some(trimmed.to_owned());
    }

    if looks_like_scheme_less_url(trimmed) {
        return Some(format!("https://{trimmed}"));
    }

    None
}

fn trim_url_trailing_punctuation(mut text: &str) -> &str {
    while let Some(ch) = text.chars().next_back() {
        if !is_trailing_url_punctuation(ch) {
            break;
        }
        text = &text[..text.len() - ch.len_utf8()];
    }
    text
}

fn is_scheme_url(text: &str) -> bool {
    text.starts_with("http://") || text.starts_with("https://")
}

fn looks_like_scheme_less_url(text: &str) -> bool {
    if text.contains('@') || text.starts_with('.') || text.ends_with('.') {
        return false;
    }

    let Some(host) = text.split(['/', '?', '#']).next() else {
        return false;
    };
    if !host.contains('.') {
        return false;
    }

    host.split('.').all(is_valid_domain_label)
}

fn is_valid_domain_label(label: &str) -> bool {
    !label.is_empty()
        && label
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        && !label.starts_with('-')
        && !label.ends_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::terminal::Terminal;

    fn put_ascii(terminal: &mut Terminal, row: usize, col: usize, text: &str) {
        terminal.set_cursor(row, col);
        for ch in text.chars() {
            terminal.put_char(ch);
        }
    }

    #[test]
    fn selection_extracts_single_line_text() {
        let mut terminal = Terminal::new(2, 8);
        put_ascii(&mut terminal, 0, 0, "hello");

        let snap = terminal.snapshot();
        let text = selected_text(
            &snap,
            Selection {
                start: GridPoint { row: 0, col: 1 },
                end: GridPoint { row: 0, col: 3 },
            },
        );

        assert_eq!(text, "ell");
    }

    #[test]
    fn selection_extracts_multiline_text_and_trims_right_spaces() {
        let mut terminal = Terminal::new(3, 8);
        put_ascii(&mut terminal, 0, 0, "ab  ");
        put_ascii(&mut terminal, 1, 0, "cd  ");

        let snap = terminal.snapshot();
        let text = selected_text(
            &snap,
            Selection {
                start: GridPoint { row: 0, col: 0 },
                end: GridPoint { row: 1, col: 3 },
            },
        );

        assert_eq!(text, "ab\ncd");
    }

    #[test]
    fn selection_skips_double_width_continuation_cells() {
        let mut terminal = Terminal::new(1, 8);
        terminal.set_cursor(0, 0);
        terminal.put_char('中');
        terminal.put_char('A');

        let snap = terminal.snapshot();
        let text = selected_text(
            &snap,
            Selection {
                start: GridPoint { row: 0, col: 0 },
                end: GridPoint { row: 0, col: 2 },
            },
        );

        assert_eq!(text, "中A");
    }

    #[test]
    fn url_at_grid_point_detects_http_url_on_same_line() {
        let mut terminal = Terminal::new(2, 40);
        put_ascii(&mut terminal, 0, 3, "see https://example.com/path now");

        let snap = terminal.snapshot();
        let url = url_at_grid_point(&snap, GridPoint { row: 0, col: 12 });

        assert_eq!(url.as_deref(), Some("https://example.com/path"));
    }

    #[test]
    fn url_at_grid_point_trims_trailing_sentence_punctuation() {
        let mut terminal = Terminal::new(1, 40);
        put_ascii(&mut terminal, 0, 0, "https://example.com/test).");

        let snap = terminal.snapshot();
        let url = url_at_grid_point(&snap, GridPoint { row: 0, col: 5 });

        assert_eq!(url.as_deref(), Some("https://example.com/test"));
    }

    #[test]
    fn url_at_grid_point_ignores_non_url_cells() {
        let mut terminal = Terminal::new(1, 40);
        put_ascii(&mut terminal, 0, 0, "plain https://example.com");

        let snap = terminal.snapshot();
        assert_eq!(url_at_grid_point(&snap, GridPoint { row: 0, col: 2 }), None);
    }

    #[test]
    fn selected_url_without_scheme_gets_https_scheme() {
        assert_eq!(
            normalize_selected_url_for_open("www.example.com/path"),
            Some("https://www.example.com/path".to_owned())
        );
        assert_eq!(
            normalize_selected_url_for_open("example.com/path"),
            Some("https://example.com/path".to_owned())
        );
    }

    #[test]
    fn selected_url_keeps_existing_http_scheme() {
        assert_eq!(
            normalize_selected_url_for_open("http://example.com"),
            Some("http://example.com".to_owned())
        );
        assert_eq!(
            normalize_selected_url_for_open("https://example.com"),
            Some("https://example.com".to_owned())
        );
    }

    #[test]
    fn selected_url_rejects_plain_words_and_email_addresses() {
        assert_eq!(normalize_selected_url_for_open("example"), None);
        assert_eq!(normalize_selected_url_for_open("user@example.com"), None);
    }
}
