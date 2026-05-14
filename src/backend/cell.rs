#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub width: u8,
    pub fg_color: Color,
    pub bg_color: Color,
    pub bold: bool,
    pub underline: bool,
    pub blink: bool,
    pub reverse: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            width: 1,
            fg_color: Color::Default,
            bg_color: Color::Default,
            bold: false,
            underline: false,
            blink: false,
            reverse: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    /// Returns the 8-bit RGB triple this color renders as. UI-toolkit-neutral.
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Color::Default => (255, 255, 255),
            Color::Black => (0, 0, 0),
            Color::Red => (205, 0, 0),
            Color::Green => (0, 205, 0),
            Color::Yellow => (205, 205, 0),
            Color::Blue => (0, 0, 238),
            Color::Magenta => (205, 0, 205),
            Color::Cyan => (0, 205, 205),
            Color::White => (229, 229, 229),
            Color::BrightBlack => (127, 127, 127),
            Color::BrightRed => (255, 0, 0),
            Color::BrightGreen => (0, 255, 0),
            Color::BrightYellow => (255, 255, 0),
            Color::BrightBlue => (92, 92, 255),
            Color::BrightMagenta => (255, 0, 255),
            Color::BrightCyan => (0, 255, 255),
            Color::BrightWhite => (255, 255, 255),
            Color::Indexed(i) => Self::indexed_rgb(i),
            Color::Rgb(r, g, b) => (r, g, b),
        }
    }

    fn indexed_rgb(index: u8) -> (u8, u8, u8) {
        match index {
            0..=15 => {
                let colors = [
                    Color::Black,
                    Color::Red,
                    Color::Green,
                    Color::Yellow,
                    Color::Blue,
                    Color::Magenta,
                    Color::Cyan,
                    Color::White,
                    Color::BrightBlack,
                    Color::BrightRed,
                    Color::BrightGreen,
                    Color::BrightYellow,
                    Color::BrightBlue,
                    Color::BrightMagenta,
                    Color::BrightCyan,
                    Color::BrightWhite,
                ];
                colors[index as usize].rgb()
            }
            16..=231 => {
                let idx = index - 16;
                let r = (idx / 36) * 51;
                let g = ((idx % 36) / 6) * 51;
                let b = (idx % 6) * 51;
                (r, g, b)
            }
            232..=255 => {
                let gray = (index - 232) * 10 + 8;
                (gray, gray, gray)
            }
        }
    }
}
