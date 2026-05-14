use crate::backend::cell::{Cell, Color};

pub struct Terminal {
    pub rows: usize,
    pub cols: usize,
    pub grid: Vec<Vec<Cell>>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub saved_cursor_row: usize,
    pub saved_cursor_col: usize,
    pub scroll_top: usize,
    pub scroll_bottom: usize,
    pub fg_color: Color,
    pub bg_color: Color,
    pub bold: bool,
    pub underline: bool,
    pub blink: bool,
    pub reverse: bool,
    pub auto_wrap: bool,
    pub origin_mode: bool,
    pub insert_mode: bool,
    pub line_feed_new_line_mode: bool,
    pub dirty: bool,
}

impl Terminal {
    pub fn new(rows: usize, cols: usize) -> Self {
        let mut grid = Vec::with_capacity(rows);
        for _ in 0..rows {
            let mut row = Vec::with_capacity(cols);
            for _ in 0..cols {
                row.push(Cell::default());
            }
            grid.push(row);
        }

        Self {
            rows,
            cols,
            grid,
            cursor_row: 0,
            cursor_col: 0,
            saved_cursor_row: 0,
            saved_cursor_col: 0,
            scroll_top: 0,
            scroll_bottom: rows - 1,
            fg_color: Color::Default,
            bg_color: Color::Default,
            bold: false,
            underline: false,
            blink: false,
            reverse: false,
            auto_wrap: true,
            origin_mode: false,
            insert_mode: false,
            line_feed_new_line_mode: false,
            dirty: true,
        }
    }

    pub fn clear_all(&mut self) {
        for row in &mut self.grid {
            for cell in row {
                *cell = Cell::default();
            }
        }
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.dirty = true;
    }

    pub fn clear_line(&mut self, mode: ClearLineMode) {
        let row = self.cursor_row;
        match mode {
            ClearLineMode::FromCursor => {
                self.clear_cell_range(row, self.cursor_col, self.cols);
            }
            ClearLineMode::ToCursor => {
                self.clear_cell_range(row, 0, self.cursor_col.saturating_add(1));
            }
            ClearLineMode::All => {
                self.clear_cell_range(row, 0, self.cols);
            }
        }
        self.dirty = true;
    }

    pub fn clear_screen(&mut self, mode: ClearScreenMode) {
        match mode {
            ClearScreenMode::FromCursor => {
                self.clear_cell_range(self.cursor_row, self.cursor_col, self.cols);
                for row in (self.cursor_row + 1)..self.rows {
                    self.clear_cell_range(row, 0, self.cols);
                }
            }
            ClearScreenMode::ToCursor => {
                for row in 0..self.cursor_row {
                    self.clear_cell_range(row, 0, self.cols);
                }
                self.clear_cell_range(self.cursor_row, 0, self.cursor_col.saturating_add(1));
            }
            ClearScreenMode::All => {
                self.clear_all();
            }
        }
        self.dirty = true;
    }

    pub fn put_char(&mut self, ch: char) {
        if self.cursor_row >= self.rows {
            return;
        }

        let width = char_width(ch);

        if self.cursor_col >= self.cols || self.cursor_col + width as usize > self.cols {
            if !self.auto_wrap {
                self.cursor_col = self.cols.saturating_sub(1);
                return;
            }

            self.cursor_col = 0;
            if self.cursor_row == self.scroll_bottom {
                self.scroll_up(1);
            } else if self.cursor_row < self.rows - 1 {
                self.cursor_row += 1;
            }
        }

        if self.cursor_col + width as usize > self.cols {
            return;
        }

        if self.insert_mode {
            self.insert_char(ch, width);
        } else {
            let row = self.cursor_row;
            let col = self.cursor_col;
            self.clear_cell_range(row, col, col + width as usize);
            self.write_cell(row, col, ch, width);
            self.advance_cursor(width);
        }
        self.dirty = true;
    }

    pub fn insert_char(&mut self, ch: char, width: u8) {
        let row = self.cursor_row;
        let col = self.cursor_col;
        let width = width as usize;
        if col + width > self.cols {
            return;
        }

        for c in (col..(self.cols - width)).rev() {
            self.grid[row][c + width] = self.grid[row][c];
        }
        let blank = self.blank_cell_for_current_background();
        for c in col..(col + width) {
            self.grid[row][c] = blank;
        }
        self.write_cell(row, col, ch, width as u8);
        self.sanitize_row(row);
        self.advance_cursor(width as u8);
    }

    fn write_cell(&mut self, row: usize, col: usize, ch: char, width: u8) {
        self.grid[row][col] = Cell {
            ch,
            width,
            fg_color: self.fg_color,
            bg_color: self.bg_color,
            bold: self.bold,
            underline: self.underline,
            blink: self.blink,
            reverse: self.reverse,
        };
        for i in 1..width as usize {
            if col + i < self.cols {
                self.grid[row][col + i] = Cell {
                    ch: '\0',
                    width: 0,
                    fg_color: self.fg_color,
                    bg_color: self.bg_color,
                    bold: self.bold,
                    underline: self.underline,
                    blink: self.blink,
                    reverse: self.reverse,
                };
            }
        }
    }

    fn blank_cell_for_current_background(&self) -> Cell {
        Cell {
            bg_color: self.bg_color,
            ..Default::default()
        }
    }

    fn clear_cell_range(&mut self, row: usize, start: usize, end: usize) {
        let (start, end) = self.expand_range_for_wide_cells(row, start, end);
        let blank = self.blank_cell_for_current_background();
        for col in start..end {
            self.grid[row][col] = blank;
        }
    }

    fn expand_range_for_wide_cells(&self, row: usize, start: usize, end: usize) -> (usize, usize) {
        let mut start = start.min(self.cols);
        let mut end = end.min(self.cols);

        if start < end && start > 0 && self.grid[row][start].width == 0 {
            start -= 1;
        }
        if start < end && end < self.cols && self.grid[row][end].width == 0 {
            end += 1;
        }

        (start, end)
    }

    fn sanitize_row(&mut self, row: usize) {
        let mut col = 0;
        while col < self.cols {
            match self.grid[row][col].width {
                0 => {
                    if col == 0 || self.grid[row][col - 1].width <= 1 {
                        self.grid[row][col] = Cell::default();
                    }
                    col += 1;
                }
                width if width as usize > 1 => {
                    let end = col + width as usize;
                    if end > self.cols {
                        self.grid[row][col] = Cell::default();
                        col += 1;
                    } else {
                        col = end;
                    }
                }
                _ => col += 1,
            }
        }
    }

    fn advance_cursor(&mut self, width: u8) {
        self.cursor_col += width as usize;
        if self.cursor_col > self.cols {
            self.cursor_col = self.cols;
        }
    }

    pub fn set_cursor(&mut self, row: usize, col: usize) {
        let mut row = row;
        let mut col = col;
        if col >= self.cols {
            col = self.cols.saturating_sub(1);
        }
        if row >= self.rows {
            row = self.rows.saturating_sub(1);
        }
        self.cursor_row = row;
        self.cursor_col = col;
    }

    pub fn move_cursor_up(&mut self, n: usize) {
        if self.origin_mode {
            self.cursor_row = self.cursor_row.saturating_sub(n);
            if self.cursor_row < self.scroll_top {
                self.cursor_row = self.scroll_top;
            }
        } else {
            self.cursor_row = self.cursor_row.saturating_sub(n);
        }
    }

    pub fn move_cursor_down(&mut self, n: usize) {
        if self.origin_mode {
            self.cursor_row = (self.cursor_row + n).min(self.scroll_bottom);
        } else {
            self.cursor_row = (self.cursor_row + n).min(self.rows - 1);
        }
    }

    pub fn move_cursor_forward(&mut self, n: usize) {
        self.cursor_col = (self.cursor_col + n).min(self.cols - 1);
    }

    pub fn move_cursor_back(&mut self, n: usize) {
        self.cursor_col = self.cursor_col.saturating_sub(n);
    }

    pub fn carriage_return(&mut self) {
        self.cursor_col = 0;
    }

    pub fn line_feed(&mut self) {
        if self.line_feed_new_line_mode {
            self.cursor_col = 0;
        }
        if self.cursor_row == self.scroll_bottom {
            self.scroll_up(1);
        } else if self.cursor_row < self.rows - 1 {
            self.cursor_row += 1;
        }
        self.dirty = true;
    }

    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        }
    }

    pub fn tab(&mut self) {
        let next_tab = (self.cursor_col / 8 + 1) * 8;
        self.cursor_col = next_tab.min(self.cols - 1);
    }

    pub fn save_cursor(&mut self) {
        self.saved_cursor_row = self.cursor_row;
        self.saved_cursor_col = self.cursor_col;
    }

    pub fn restore_cursor(&mut self) {
        self.cursor_row = self.saved_cursor_row;
        self.cursor_col = self.saved_cursor_col;
    }

    pub fn scroll_up(&mut self, n: usize) {
        let n = n.min(self.scroll_bottom - self.scroll_top + 1);
        for _ in 0..n {
            let removed = self.grid.remove(self.scroll_top);
            let mut new_row = Vec::with_capacity(self.cols);
            for _ in 0..self.cols {
                new_row.push(Cell::default());
            }
            self.grid.insert(self.scroll_bottom, new_row);
            drop(removed);
        }
        self.dirty = true;
    }

    pub fn scroll_down(&mut self, n: usize) {
        let n = n.min(self.scroll_bottom - self.scroll_top + 1);
        for _ in 0..n {
            self.grid.remove(self.scroll_bottom);
            let mut new_row = Vec::with_capacity(self.cols);
            for _ in 0..self.cols {
                new_row.push(Cell::default());
            }
            self.grid.insert(self.scroll_top, new_row);
        }
        self.dirty = true;
    }

    pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        self.scroll_top = top.min(self.rows - 1);
        self.scroll_bottom = bottom.min(self.rows - 1);
        if self.scroll_top >= self.scroll_bottom {
            self.scroll_top = 0;
            self.scroll_bottom = self.rows - 1;
        }
    }

    pub fn reset_attributes(&mut self) {
        self.fg_color = Color::Default;
        self.bg_color = Color::Default;
        self.bold = false;
        self.underline = false;
        self.blink = false;
        self.reverse = false;
    }

    pub fn set_attribute(&mut self, attr: Attribute) {
        match attr {
            Attribute::Reset => self.reset_attributes(),
            Attribute::Bold => self.bold = true,
            Attribute::Underline => self.underline = true,
            Attribute::Blink => self.blink = true,
            Attribute::Reverse => self.reverse = true,
            Attribute::Foreground(c) => self.fg_color = c,
            Attribute::Background(c) => self.bg_color = c,
        }
    }

    pub fn erase_chars(&mut self, n: usize) {
        let row = self.cursor_row;
        let start = self.cursor_col;
        let end = (start + n).min(self.cols);
        self.clear_cell_range(row, start, end);
        self.dirty = true;
    }

    pub fn delete_chars(&mut self, n: usize) {
        let row = self.cursor_row;
        let col = self.cursor_col;
        let end = (col + n).min(self.cols);
        let (col, end) = self.expand_range_for_wide_cells(row, col, end);
        let n = end.saturating_sub(col);
        for c in col..(self.cols - n) {
            self.grid[row][c] = self.grid[row][c + n];
        }
        let blank = self.blank_cell_for_current_background();
        for c in (self.cols - n)..self.cols {
            self.grid[row][c] = blank;
        }
        self.sanitize_row(row);
        self.dirty = true;
    }

    pub fn delete_lines(&mut self, n: usize) {
        let n = n.min(self.scroll_bottom - self.cursor_row + 1);
        for _ in 0..n {
            self.grid.remove(self.cursor_row);
            let mut new_row = Vec::with_capacity(self.cols);
            for _ in 0..self.cols {
                new_row.push(Cell::default());
            }
            self.grid.insert(self.scroll_bottom, new_row);
        }
        self.dirty = true;
    }

    pub fn insert_lines(&mut self, n: usize) {
        let n = n.min(self.scroll_bottom - self.cursor_row + 1);
        for _ in 0..n {
            self.grid.remove(self.scroll_bottom);
            let mut new_row = Vec::with_capacity(self.cols);
            for _ in 0..self.cols {
                new_row.push(Cell::default());
            }
            self.grid.insert(self.cursor_row, new_row);
        }
        self.dirty = true;
    }
}

fn char_width(ch: char) -> u8 {
    if is_welly_ascii_art_symbol(ch) {
        2
    } else {
        unicode_width::UnicodeWidthChar::width_cjk(ch)
            .unwrap_or(1)
            .max(1) as u8
    }
}

fn is_welly_ascii_art_symbol(ch: char) -> bool {
    matches!(
        ch,
        '◼'
            | '◆'
            | '◇'
            | '▁'..='█'
            | '▉'..='▏'
            | '▔'
            | '▕'
            | '◢'..='◥'
            | '╱'..='╳'
            | '═'..='╬'
            | '╭'..='╰'
            | '┌'..='╋'
            | '─'..='┃'
            | '—'
            | '︳'
            | '￣'
            | '＿'
            | '／'
            | '﹨'
            | '＼'
    )
}

#[cfg(test)]
mod tests {
    use super::Terminal;
    use crate::backend::cell::Color;

    #[test]
    fn ascii_art_symbols_occupy_two_cells() {
        let mut terminal = Terminal::new(1, 4);

        terminal.put_char('┌');

        assert_eq!(terminal.cursor_col, 2);
        assert_eq!(terminal.grid[0][0].ch, '┌');
        assert_eq!(terminal.grid[0][0].width, 2);
        assert_eq!(terminal.grid[0][1].ch, '\0');
        assert_eq!(terminal.grid[0][1].width, 0);
    }

    #[test]
    fn block_triangle_and_diagonal_symbols_occupy_two_cells() {
        for ch in ['█', '▌', '◆', '◇', '◢', '╱'] {
            let mut terminal = Terminal::new(1, 4);

            terminal.put_char(ch);

            assert_eq!(terminal.cursor_col, 2, "{ch} should advance by two cells");
            assert_eq!(terminal.grid[0][0].ch, ch);
            assert_eq!(terminal.grid[0][0].width, 2);
            assert_eq!(terminal.grid[0][1].width, 0);
        }
    }

    #[test]
    fn cjk_punctuation_occupies_two_cells() {
        for ch in ['“', '”', '‘', '’', '…'] {
            let mut terminal = Terminal::new(1, 4);

            terminal.put_char(ch);

            assert_eq!(terminal.cursor_col, 2, "{ch} should advance by two cells");
            assert_eq!(terminal.grid[0][0].ch, ch);
            assert_eq!(terminal.grid[0][0].width, 2);
            assert_eq!(terminal.grid[0][1].ch, '\0');
            assert_eq!(terminal.grid[0][1].width, 0);
        }
    }

    #[test]
    fn filling_last_column_does_not_immediately_wrap() {
        let mut terminal = Terminal::new(2, 4);

        for ch in ['a', 'b', 'c', 'd'] {
            terminal.put_char(ch);
        }

        assert_eq!(terminal.cursor_row, 0);
        assert_eq!(terminal.cursor_col, 4);
        assert_eq!(terminal.grid[0][3].ch, 'd');
        assert_eq!(terminal.grid[1][0].ch, ' ');
    }

    #[test]
    fn line_feed_after_full_line_advances_only_one_row() {
        let mut terminal = Terminal::new(3, 4);

        for ch in ['a', 'b', 'c', 'd'] {
            terminal.put_char(ch);
        }
        terminal.line_feed();

        assert_eq!(terminal.cursor_row, 1);
        assert_eq!(terminal.cursor_col, 4);
        assert_eq!(terminal.grid[0][3].ch, 'd');
        assert_eq!(terminal.grid[2][0].ch, ' ');
    }

    #[test]
    fn overwriting_double_width_continuation_clears_leading_cell() {
        let mut terminal = Terminal::new(1, 4);

        terminal.put_char('表');
        terminal.set_cursor(0, 1);
        terminal.put_char('都');

        assert_eq!(terminal.grid[0][0], Default::default());
        assert_eq!(terminal.grid[0][1].ch, '都');
        assert_eq!(terminal.grid[0][1].width, 2);
        assert_eq!(terminal.grid[0][2].ch, '\0');
        assert_eq!(terminal.grid[0][2].width, 0);
    }

    #[test]
    fn overwriting_double_width_leading_cell_clears_continuation() {
        let mut terminal = Terminal::new(1, 4);

        terminal.put_char('表');
        terminal.set_cursor(0, 0);
        terminal.put_char('A');

        assert_eq!(terminal.grid[0][0].ch, 'A');
        assert_eq!(terminal.grid[0][0].width, 1);
        assert_eq!(terminal.grid[0][1], Default::default());
    }

    #[test]
    fn clear_line_preserves_current_background_on_blanks() {
        let mut terminal = Terminal::new(1, 6);

        terminal.bg_color = Color::Red;
        terminal.put_char('M');
        terminal.put_char('a');
        terminal.set_cursor(0, 0);
        terminal.clear_line(super::ClearLineMode::All);

        assert_eq!(terminal.grid[0][0].ch, ' ');
        assert_eq!(terminal.grid[0][0].bg_color, Color::Red);
        assert_eq!(terminal.grid[0][1].bg_color, Color::Red);
    }

    #[test]
    fn delete_chars_preserves_current_background_on_revealed_blanks() {
        let mut terminal = Terminal::new(1, 5);

        terminal.bg_color = Color::Red;
        for ch in "abcde".chars() {
            terminal.put_char(ch);
        }
        terminal.set_cursor(0, 1);
        terminal.delete_chars(2);

        assert_eq!(row_text(&terminal, 0), "ade  ");
        assert_eq!(terminal.grid[0][3].bg_color, Color::Red);
        assert_eq!(terminal.grid[0][4].bg_color, Color::Red);
    }

    #[test]
    fn overwriting_double_width_half_preserves_current_background() {
        let mut terminal = Terminal::new(1, 4);

        terminal.bg_color = Color::Red;
        terminal.put_char('表');
        terminal.set_cursor(0, 1);
        terminal.put_char('A');

        assert_eq!(terminal.grid[0][0].ch, ' ');
        assert_eq!(terminal.grid[0][0].bg_color, Color::Red);
        assert_eq!(terminal.grid[0][1].ch, 'A');
        assert_eq!(terminal.grid[0][1].bg_color, Color::Red);
    }

    fn row_text(terminal: &Terminal, row: usize) -> String {
        terminal.grid[row].iter().map(|cell| cell.ch).collect()
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ClearLineMode {
    FromCursor,
    ToCursor,
    All,
}

#[derive(Clone, Copy, Debug)]
pub enum ClearScreenMode {
    FromCursor,
    ToCursor,
    All,
}

#[derive(Clone, Debug)]
pub enum Attribute {
    Reset,
    Bold,
    Underline,
    Blink,
    Reverse,
    Foreground(Color),
    Background(Color),
}
