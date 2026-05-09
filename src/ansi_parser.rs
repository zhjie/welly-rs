use crate::cell::Color;
use crate::terminal::{Attribute, ClearLineMode, ClearScreenMode, Terminal};
use encoding_rs::{CoderResult, Decoder, GB18030};

#[derive(Clone, Copy, Debug, PartialEq)]
enum ParserState {
    Normal,
    Escape,
    Csi,
    Osc,
    OscEnd,
    Charset,
}

pub struct AnsiParser {
    state: ParserState,
    decoder: Decoder,
    params: Vec<i64>,
    current_param: i64,
    param_has_value: bool,
    private_mode: bool,
    osc_string: String,
    charset: u8,
}

impl AnsiParser {
    pub fn new() -> Self {
        Self {
            state: ParserState::Normal,
            decoder: GB18030.new_decoder_without_bom_handling(),
            params: Vec::new(),
            current_param: 0,
            param_has_value: false,
            private_mode: false,
            osc_string: String::new(),
            charset: b'B',
        }
    }

    pub fn feed(&mut self, data: &[u8], terminal: &mut Terminal) {
        let decoded = self.decode_text(data);
        for ch in decoded.chars() {
            self.process_char(ch, terminal);
        }
    }

    pub fn feed_bytes(&mut self, data: &[u8], terminal: &mut Terminal) {
        self.feed(data, terminal);
    }

    fn process_char(&mut self, ch: char, terminal: &mut Terminal) {
        match self.state {
            ParserState::Normal => self.process_normal(ch, terminal),
            ParserState::Escape => self.process_escape(ch, terminal),
            ParserState::Csi => self.process_csi(ch, terminal),
            ParserState::Osc => self.process_osc(ch, terminal),
            ParserState::OscEnd => self.process_osc_end(ch, terminal),
            ParserState::Charset => self.process_charset(ch),
        }
    }

    fn process_normal(&mut self, ch: char, terminal: &mut Terminal) {
        match ch {
            '\u{1b}' => {
                self.state = ParserState::Escape;
                self.clear_params();
            }
            '\u{07}' => {}
            '\u{08}' => terminal.backspace(),
            '\u{09}' => terminal.tab(),
            '\u{0a}' | '\u{0b}' | '\u{0c}' => terminal.line_feed(),
            '\u{0d}' => terminal.carriage_return(),
            '\u{0e}' => {}
            '\u{0f}' => {}
            ch => {
                if !ch.is_control() {
                    terminal.put_char(self.map_charset_char(ch));
                }
            }
        }
    }

    fn process_escape(&mut self, ch: char, terminal: &mut Terminal) {
        match ch {
            '[' => {
                self.state = ParserState::Csi;
                self.clear_params();
            }
            ']' => {
                self.state = ParserState::Osc;
                self.osc_string.clear();
            }
            '(' | ')' | '*' | '+' => {
                self.state = ParserState::Charset;
            }
            '7' => {
                terminal.save_cursor();
                self.state = ParserState::Normal;
            }
            '8' => {
                terminal.restore_cursor();
                self.state = ParserState::Normal;
            }
            'D' => {
                terminal.line_feed();
                self.state = ParserState::Normal;
            }
            'E' => {
                terminal.carriage_return();
                terminal.line_feed();
                self.state = ParserState::Normal;
            }
            'H' => {
                self.state = ParserState::Normal;
            }
            'M' => {
                if terminal.cursor_row > terminal.scroll_top {
                    terminal.cursor_row -= 1;
                } else {
                    terminal.scroll_down(1);
                }
                self.state = ParserState::Normal;
            }
            'c' => {
                terminal.clear_all();
                terminal.reset_attributes();
                terminal.scroll_top = 0;
                terminal.scroll_bottom = terminal.rows - 1;
                self.state = ParserState::Normal;
            }
            _ => {
                self.state = ParserState::Normal;
            }
        }
    }

    fn process_csi(&mut self, ch: char, terminal: &mut Terminal) {
        if is_csi_final_byte(ch) {
            self.finish_params();
        }

        match ch {
            '0'..='9' => {
                let digit = ch as i64 - '0' as i64;
                self.current_param = self.current_param * 10 + digit;
                self.param_has_value = true;
            }
            ';' | ':' => {
                self.push_param();
            }
            '?' => {
                self.private_mode = true;
            }
            'A' => {
                let n = self.get_param(0, 1) as usize;
                terminal.move_cursor_up(n);
                self.state = ParserState::Normal;
            }
            'B' => {
                let n = self.get_param(0, 1) as usize;
                terminal.move_cursor_down(n);
                self.state = ParserState::Normal;
            }
            'C' => {
                let n = self.get_param(0, 1) as usize;
                terminal.move_cursor_forward(n);
                self.state = ParserState::Normal;
            }
            'D' => {
                let n = self.get_param(0, 1) as usize;
                terminal.move_cursor_back(n);
                self.state = ParserState::Normal;
            }
            'E' => {
                let n = self.get_param(0, 1) as usize;
                terminal.cursor_row = (terminal.cursor_row + n).min(terminal.rows - 1);
                terminal.cursor_col = 0;
                self.state = ParserState::Normal;
            }
            'F' => {
                let n = self.get_param(0, 1) as usize;
                terminal.cursor_row = terminal.cursor_row.saturating_sub(n);
                terminal.cursor_col = 0;
                self.state = ParserState::Normal;
            }
            'G' => {
                let col = self.get_param(0, 1) as usize;
                terminal.set_cursor(terminal.cursor_row, col.saturating_sub(1));
                self.state = ParserState::Normal;
            }
            'H' | 'f' => {
                let row = self.get_param(0, 1) as usize;
                let col = self.get_param(1, 1) as usize;
                let actual_row = if terminal.origin_mode {
                    terminal.scroll_top + row.saturating_sub(1)
                } else {
                    row.saturating_sub(1)
                };
                terminal.set_cursor(actual_row, col.saturating_sub(1));
                self.state = ParserState::Normal;
            }
            'I' => {
                let n = self.get_param(0, 1) as usize;
                for _ in 0..n {
                    terminal.tab();
                }
                self.state = ParserState::Normal;
            }
            'J' => {
                let mode = match self.get_param(0, 0) {
                    0 => ClearScreenMode::FromCursor,
                    1 => ClearScreenMode::ToCursor,
                    2 => ClearScreenMode::All,
                    _ => ClearScreenMode::FromCursor,
                };
                terminal.clear_screen(mode);
                self.state = ParserState::Normal;
            }
            'K' => {
                let mode = match self.get_param(0, 0) {
                    0 => ClearLineMode::FromCursor,
                    1 => ClearLineMode::ToCursor,
                    2 => ClearLineMode::All,
                    _ => ClearLineMode::FromCursor,
                };
                terminal.clear_line(mode);
                self.state = ParserState::Normal;
            }
            'L' => {
                let n = self.get_param(0, 1) as usize;
                terminal.insert_lines(n);
                self.state = ParserState::Normal;
            }
            'M' => {
                let n = self.get_param(0, 1) as usize;
                terminal.delete_lines(n);
                self.state = ParserState::Normal;
            }
            'P' => {
                let n = self.get_param(0, 1) as usize;
                terminal.delete_chars(n);
                self.state = ParserState::Normal;
            }
            'S' => {
                let n = self.get_param(0, 1) as usize;
                terminal.scroll_up(n);
                self.state = ParserState::Normal;
            }
            'T' => {
                let n = self.get_param(0, 1) as usize;
                terminal.scroll_down(n);
                self.state = ParserState::Normal;
            }
            'X' => {
                let n = self.get_param(0, 1) as usize;
                terminal.erase_chars(n);
                self.state = ParserState::Normal;
            }
            'Z' => {
                let n = self.get_param(0, 1) as usize;
                for _ in 0..n {
                    let prev_tab = ((terminal.cursor_col / 8).saturating_sub(1)) * 8;
                    terminal.cursor_col = prev_tab;
                }
                self.state = ParserState::Normal;
            }
            'd' => {
                let row = self.get_param(0, 1) as usize;
                let actual_row = if terminal.origin_mode {
                    terminal.scroll_top + row.saturating_sub(1)
                } else {
                    row.saturating_sub(1)
                };
                terminal.set_cursor(actual_row, terminal.cursor_col);
                self.state = ParserState::Normal;
            }
            'h' => {
                self.process_mode_set(terminal, true);
                self.state = ParserState::Normal;
            }
            'l' => {
                self.process_mode_set(terminal, false);
                self.state = ParserState::Normal;
            }
            'm' => {
                self.process_sgr(terminal);
                self.state = ParserState::Normal;
            }
            'r' => {
                let top = self.get_param(0, 1) as usize;
                let bottom = self.get_param(1, terminal.rows as i64) as usize;
                terminal.set_scroll_region(top.saturating_sub(1), bottom.saturating_sub(1));
                terminal.set_cursor(terminal.scroll_top, 0);
                self.state = ParserState::Normal;
            }
            's' => {
                terminal.save_cursor();
                self.state = ParserState::Normal;
            }
            'u' => {
                terminal.restore_cursor();
                self.state = ParserState::Normal;
            }
            '@' => {
                let n = self.get_param(0, 1) as usize;
                for _ in 0..n {
                    terminal.insert_char(' ', 1);
                }
                self.state = ParserState::Normal;
            }
            '\u{1b}' => {
                self.clear_params();
            }
            _ => {
                if ch.is_ascii_alphabetic() {
                    self.state = ParserState::Normal;
                }
            }
        }
    }

    fn process_osc(&mut self, ch: char, _terminal: &mut Terminal) {
        if ch == '\u{07}' {
            self.state = ParserState::Normal;
        } else if ch == '\u{1b}' {
            self.state = ParserState::OscEnd;
        } else {
            self.osc_string.push(ch);
        }
    }

    fn process_osc_end(&mut self, ch: char, _terminal: &mut Terminal) {
        if ch == '\\' || ch == '\u{07}' {}
        self.state = ParserState::Normal;
    }

    fn process_charset(&mut self, ch: char) {
        self.charset = ch as u8;
        self.state = ParserState::Normal;
    }

    fn map_charset_char(&self, ch: char) -> char {
        if self.charset != b'0' {
            return ch;
        }

        match ch {
            '`' => '◆',
            'a' => '▒',
            'b' => '␉',
            'c' => '␌',
            'd' => '␍',
            'e' => '␊',
            'f' => '°',
            'g' => '±',
            'h' => '␤',
            'i' => '␋',
            'j' => '┘',
            'k' => '┐',
            'l' => '┌',
            'm' => '└',
            'n' => '┼',
            'o' => '⎺',
            'p' => '⎻',
            'q' => '─',
            'r' => '⎼',
            's' => '⎽',
            't' => '├',
            'u' => '┤',
            'v' => '┴',
            'w' => '┬',
            'x' => '│',
            'y' => '≤',
            'z' => '≥',
            '{' => 'π',
            '|' => '≠',
            '}' => '£',
            '~' => '·',
            _ => ch,
        }
    }

    fn process_sgr(&mut self, terminal: &mut Terminal) {
        if self.params.is_empty() {
            terminal.set_attribute(Attribute::Reset);
            return;
        }

        let mut i = 0;
        while i < self.params.len() {
            let param = self.params[i];
            match param {
                0 => terminal.set_attribute(Attribute::Reset),
                1 => terminal.set_attribute(Attribute::Bold),
                4 => terminal.set_attribute(Attribute::Underline),
                5 => terminal.set_attribute(Attribute::Blink),
                7 => terminal.set_attribute(Attribute::Reverse),
                22 => terminal.bold = false,
                24 => terminal.underline = false,
                25 => terminal.blink = false,
                27 => terminal.reverse = false,
                30..=37 => {
                    let color = Self::sgr_color(param - 30);
                    terminal.set_attribute(Attribute::Foreground(color));
                }
                38 => {
                    if i + 2 < self.params.len() && self.params[i + 1] == 5 {
                        let color = Color::Indexed(self.params[i + 2] as u8);
                        terminal.set_attribute(Attribute::Foreground(color));
                        i += 2;
                    } else if i + 4 < self.params.len() && self.params[i + 1] == 2 {
                        let color = Color::Rgb(
                            self.params[i + 2] as u8,
                            self.params[i + 3] as u8,
                            self.params[i + 4] as u8,
                        );
                        terminal.set_attribute(Attribute::Foreground(color));
                        i += 4;
                    }
                }
                39 => terminal.set_attribute(Attribute::Foreground(Color::Default)),
                40..=47 => {
                    let color = Self::sgr_color(param - 40);
                    terminal.set_attribute(Attribute::Background(color));
                }
                48 => {
                    if i + 2 < self.params.len() && self.params[i + 1] == 5 {
                        let color = Color::Indexed(self.params[i + 2] as u8);
                        terminal.set_attribute(Attribute::Background(color));
                        i += 2;
                    } else if i + 4 < self.params.len() && self.params[i + 1] == 2 {
                        let color = Color::Rgb(
                            self.params[i + 2] as u8,
                            self.params[i + 3] as u8,
                            self.params[i + 4] as u8,
                        );
                        terminal.set_attribute(Attribute::Background(color));
                        i += 4;
                    }
                }
                49 => terminal.set_attribute(Attribute::Background(Color::Default)),
                90..=97 => {
                    let color = Self::sgr_bright_color(param - 90);
                    terminal.set_attribute(Attribute::Foreground(color));
                }
                100..=107 => {
                    let color = Self::sgr_bright_color(param - 100);
                    terminal.set_attribute(Attribute::Background(color));
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn process_mode_set(&mut self, terminal: &mut Terminal, enabled: bool) {
        for param in &self.params {
            if self.private_mode {
                match *param {
                    6 => terminal.origin_mode = enabled,
                    7 => terminal.auto_wrap = enabled,
                    _ => {}
                }
            } else {
                match *param {
                    4 => terminal.insert_mode = enabled,
                    20 => terminal.line_feed_new_line_mode = enabled,
                    _ => {}
                }
            }
        }
    }

    fn sgr_color(code: i64) -> Color {
        match code {
            0 => Color::Black,
            1 => Color::Red,
            2 => Color::Green,
            3 => Color::Yellow,
            4 => Color::Blue,
            5 => Color::Magenta,
            6 => Color::Cyan,
            7 => Color::White,
            _ => Color::Default,
        }
    }

    fn sgr_bright_color(code: i64) -> Color {
        match code {
            0 => Color::BrightBlack,
            1 => Color::BrightRed,
            2 => Color::BrightGreen,
            3 => Color::BrightYellow,
            4 => Color::BrightBlue,
            5 => Color::BrightMagenta,
            6 => Color::BrightCyan,
            7 => Color::BrightWhite,
            _ => Color::Default,
        }
    }

    fn clear_params(&mut self) {
        self.params.clear();
        self.current_param = 0;
        self.param_has_value = false;
        self.private_mode = false;
    }

    fn finish_params(&mut self) {
        if self.param_has_value || !self.params.is_empty() {
            self.push_param();
        }
    }

    fn push_param(&mut self) {
        if self.param_has_value {
            self.params.push(self.current_param);
        } else {
            self.params.push(0);
        }
        self.current_param = 0;
        self.param_has_value = false;
    }

    fn get_param(&self, index: usize, default: i64) -> i64 {
        if index < self.params.len() {
            let val = self.params[index];
            if val > 0 {
                val
            } else {
                default
            }
        } else {
            default
        }
    }

    fn decode_text(&mut self, data: &[u8]) -> String {
        let mut output = String::with_capacity(data.len());
        let mut remaining = data;

        loop {
            output.reserve(
                self.decoder
                    .max_utf8_buffer_length(remaining.len())
                    .unwrap_or(remaining.len() * 2 + 8),
            );

            let (result, read, _) = self.decoder.decode_to_string(remaining, &mut output, false);
            remaining = &remaining[read..];

            match result {
                CoderResult::InputEmpty => break,
                CoderResult::OutputFull => continue,
            }
        }

        output
    }
}

fn is_csi_final_byte(ch: char) -> bool {
    matches!(ch, '\u{40}'..='\u{7e}')
}

#[cfg(test)]
mod tests {
    use super::AnsiParser;
    use crate::terminal::Terminal;

    #[test]
    fn vertical_tab_advances_to_next_line() {
        let mut parser = AnsiParser::new();
        let mut terminal = Terminal::new(3, 10);

        parser.feed_bytes(b"A\x0bB", &mut terminal);

        assert_eq!(terminal.grid[0][0].ch, 'A');
        assert_eq!(terminal.grid[1][1].ch, 'B');
        assert_eq!(terminal.cursor_row, 1);
        assert_eq!(terminal.cursor_col, 2);
    }

    #[test]
    fn form_feed_advances_to_next_line() {
        let mut parser = AnsiParser::new();
        let mut terminal = Terminal::new(3, 10);

        parser.feed_bytes(b"A\x0cB", &mut terminal);

        assert_eq!(terminal.grid[0][0].ch, 'A');
        assert_eq!(terminal.grid[1][1].ch, 'B');
        assert_eq!(terminal.cursor_row, 1);
        assert_eq!(terminal.cursor_col, 2);
    }

    #[test]
    fn csi_set_line_feed_new_line_mode_makes_lf_return_to_column_zero() {
        let mut parser = AnsiParser::new();
        let mut terminal = Terminal::new(3, 10);

        parser.feed_bytes(b"\x1b[20hA\nB", &mut terminal);

        assert_eq!(terminal.grid[0][0].ch, 'A');
        assert_eq!(terminal.grid[1][0].ch, 'B');
        assert_eq!(terminal.cursor_row, 1);
        assert_eq!(terminal.cursor_col, 1);
    }

    #[test]
    fn csi_reset_line_feed_new_line_mode_keeps_lf_in_same_column() {
        let mut parser = AnsiParser::new();
        let mut terminal = Terminal::new(3, 10);

        parser.feed_bytes(b"\x1b[20h\x1b[20lA\nB", &mut terminal);

        assert_eq!(terminal.grid[0][0].ch, 'A');
        assert_eq!(terminal.grid[1][1].ch, 'B');
        assert_eq!(terminal.cursor_row, 1);
        assert_eq!(terminal.cursor_col, 2);
    }

    #[test]
    fn csi_scroll_region_moves_cursor_to_region_top() {
        let mut parser = AnsiParser::new();
        let mut terminal = Terminal::new(24, 80);

        parser.feed_bytes(b"\x1b[10;20r", &mut terminal);

        assert_eq!(terminal.scroll_top, 9);
        assert_eq!(terminal.scroll_bottom, 19);
        assert_eq!(terminal.cursor_row, 9);
        assert_eq!(terminal.cursor_col, 0);
    }

    #[test]
    fn csi_cursor_position_uses_final_param() {
        let mut parser = AnsiParser::new();
        let mut terminal = Terminal::new(5, 10);

        parser.feed_bytes(b"\x1b[3;4HX", &mut terminal);

        assert_eq!(terminal.grid[2][3].ch, 'X');
        assert_eq!(terminal.cursor_row, 2);
        assert_eq!(terminal.cursor_col, 4);
    }

    #[test]
    fn csi_clear_screen_uses_final_param() {
        let mut parser = AnsiParser::new();
        let mut terminal = Terminal::new(5, 10);

        parser.feed_bytes(b"ABC\x1b[2J", &mut terminal);

        assert_eq!(terminal.grid[0][0].ch, ' ');
        assert_eq!(terminal.grid[0][1].ch, ' ');
        assert_eq!(terminal.grid[0][2].ch, ' ');
        assert_eq!(terminal.cursor_row, 0);
        assert_eq!(terminal.cursor_col, 0);
    }
}
