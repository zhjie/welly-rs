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
    pub fn to_egui_color(&self) -> egui::Color32 {
        match self {
            Color::Default => egui::Color32::WHITE,
            Color::Black => egui::Color32::from_rgb(0, 0, 0),
            Color::Red => egui::Color32::from_rgb(205, 0, 0),
            Color::Green => egui::Color32::from_rgb(0, 205, 0),
            Color::Yellow => egui::Color32::from_rgb(205, 205, 0),
            Color::Blue => egui::Color32::from_rgb(0, 0, 238),
            Color::Magenta => egui::Color32::from_rgb(205, 0, 205),
            Color::Cyan => egui::Color32::from_rgb(0, 205, 205),
            Color::White => egui::Color32::from_rgb(229, 229, 229),
            Color::BrightBlack => egui::Color32::from_rgb(127, 127, 127),
            Color::BrightRed => egui::Color32::from_rgb(255, 0, 0),
            Color::BrightGreen => egui::Color32::from_rgb(0, 255, 0),
            Color::BrightYellow => egui::Color32::from_rgb(255, 255, 0),
            Color::BrightBlue => egui::Color32::from_rgb(92, 92, 255),
            Color::BrightMagenta => egui::Color32::from_rgb(255, 0, 255),
            Color::BrightCyan => egui::Color32::from_rgb(0, 255, 255),
            Color::BrightWhite => egui::Color32::from_rgb(255, 255, 255),
            Color::Indexed(i) => Self::indexed_to_rgb(*i),
            Color::Rgb(r, g, b) => egui::Color32::from_rgb(*r, *g, *b),
        }
    }

    fn indexed_to_rgb(index: u8) -> egui::Color32 {
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
                colors[index as usize].to_egui_color()
            }
            16..=231 => {
                let idx = index - 16;
                let r = (idx / 36) * 51;
                let g = ((idx % 36) / 6) * 51;
                let b = (idx % 6) * 51;
                egui::Color32::from_rgb(r, g, b)
            }
            232..=255 => {
                let gray = (index - 232) * 10 + 8;
                egui::Color32::from_rgb(gray, gray, gray)
            }
        }
    }
}
