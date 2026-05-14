use crate::backend::cell::{self, Cell};
use crate::backend::snapshot::TerminalSnapshot;
use crate::ui::egui::fonts::{
    font_for_cell, CHINESE_LEFT_MARGIN, CHINESE_TOP_MARGIN, ENGLISH_LEFT_MARGIN,
    ENGLISH_TOP_MARGIN,
};
use crate::ui::egui::selection::{pos_to_grid_point, GridPoint, Selection};
use eframe::egui;
use egui::FontFamily;

pub const CELL_WIDTH: f32 = 18.0;
pub const CELL_HEIGHT: f32 = 35.0;
pub const TERMINAL_COLS: usize = 80;
pub const TERMINAL_ROWS: usize = 24;
pub const MIN_ZOOM: f32 = 0.5;
pub const MAX_ZOOM: f32 = 3.0;

pub struct TerminalResponse {
    pub response: egui::Response,
    pub rect: egui::Rect,
    pub cell_width: f32,
    pub cell_height: f32,
    pub rows: usize,
    pub cols: usize,
}

impl TerminalResponse {
    pub fn interact_grid_point(&self) -> Option<GridPoint> {
        let pos = self.response.interact_pointer_pos()?;
        pos_to_grid_point(pos, self.rect, self.cell_width, self.cell_height, self.rows, self.cols)
    }

    pub fn hover_grid_point(&self) -> Option<GridPoint> {
        let pos = self.response.hover_pos()?;
        pos_to_grid_point(pos, self.rect, self.cell_width, self.cell_height, self.rows, self.cols)
    }
}

#[derive(Clone, Copy)]
struct TerminalPaintGeometry {
    rect: egui::Rect,
    canvas_rect: egui::Rect,
    cell_width: f32,
    cell_height: f32,
    render_scale: f32,
}

pub fn color_to_egui(color: cell::Color) -> egui::Color32 {
    let (r, g, b) = color.rgb();
    egui::Color32::from_rgb(r, g, b)
}

pub fn render_terminal(
    ui: &mut egui::Ui,
    snap: &TerminalSnapshot<'_>,
    selection: Option<Selection>,
) -> TerminalResponse {
    let available_size = ui.available_size();
    let render_scale =
        terminal_render_scale(available_size.x, available_size.y, snap.cols, snap.row_count);
    let cell_width = CELL_WIDTH * render_scale;
    let cell_height = CELL_HEIGHT * render_scale;
    let total_width = snap.cols as f32 * cell_width;
    let total_height = snap.row_count as f32 * cell_height;

    let (response, painter) = ui.allocate_painter(available_size, egui::Sense::click_and_drag());
    if response.clicked() || !response.ctx.wants_keyboard_input() {
        response.request_focus();
    }
    let terminal_rect =
        egui::Rect::from_min_size(response.rect.min, egui::vec2(total_width, total_height));
    if response.has_focus() {
        let cursor_col = snap.cursor_col.min(snap.cols.saturating_sub(1));
        let cursor_rect = egui::Rect::from_min_size(
            egui::pos2(
                terminal_rect.min.x + cursor_col as f32 * cell_width,
                terminal_rect.min.y + snap.cursor_row as f32 * cell_height,
            ),
            egui::vec2(cell_width, cell_height),
        );
        ui.ctx().output_mut(|output| {
            output.ime = Some(egui::output::IMEOutput {
                rect: terminal_rect,
                cursor_rect,
            });
        });
    }
    let geometry = TerminalPaintGeometry {
        rect: terminal_rect,
        canvas_rect: response.rect,
        cell_width,
        cell_height,
        render_scale,
    };
    paint_terminal(snap, geometry, painter, selection);

    TerminalResponse {
        response,
        rect: terminal_rect,
        cell_width,
        cell_height,
        rows: TERMINAL_ROWS,
        cols: TERMINAL_COLS,
    }
}

fn paint_selection(
    snap: &TerminalSnapshot<'_>,
    selection: Selection,
    rect: egui::Rect,
    painter: &egui::Painter,
    cell_width: f32,
    cell_height: f32,
) {
    let (start, end) = selection.normalized();
    let color = egui::Color32::from_rgba_premultiplied(120, 170, 255, 90);

    for row in start.row..=end.row.min(snap.row_count.saturating_sub(1)) {
        let start_col = if row == start.row { start.col } else { 0 };
        let end_col = if row == end.row {
            end.col
        } else {
            snap.cols.saturating_sub(1)
        };
        let left = rect.min.x + start_col as f32 * cell_width;
        let top = rect.min.y + row as f32 * cell_height;
        let width = (end_col.saturating_sub(start_col) + 1) as f32 * cell_width;
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(left, top), egui::vec2(width, cell_height)),
            0.0,
            color,
        );
    }
}

fn paint_terminal(
    snap: &TerminalSnapshot<'_>,
    geometry: TerminalPaintGeometry,
    painter: egui::Painter,
    selection: Option<Selection>,
) {
    painter.rect_filled(geometry.canvas_rect, 0.0, egui::Color32::BLACK);
    paint_terminal_edge_bleed(
        snap,
        geometry.rect,
        geometry.canvas_rect,
        &painter,
        geometry.cell_width,
        geometry.cell_height,
    );
    if let Some(sel) = selection {
        paint_selection(snap, sel, geometry.rect, &painter, geometry.cell_width, geometry.cell_height);
    }

    for row in 0..snap.row_count {
        for col in 0..snap.cols {
            let cell = &snap.rows[row][col];
            if cell.width == 0 {
                continue;
            }

            let x = geometry.rect.min.x + col as f32 * geometry.cell_width;
            let y = geometry.rect.min.y + row as f32 * geometry.cell_height;

            let bg_color = cell_background_color(cell);

            if cell.bg_color != cell::Color::Default || cell.reverse {
                let bg_width = geometry.cell_width * cell.width as f32;
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(x, y),
                        egui::vec2(bg_width, geometry.cell_height),
                    ),
                    0.0,
                    bg_color,
                );
            }

            let fg_color = cell_foreground_color(cell);

            let cell_rect = egui::Rect::from_min_size(
                egui::pos2(x, y),
                egui::vec2(
                    geometry.cell_width * cell.width as f32,
                    geometry.cell_height,
                ),
            );
            if draw_welly_box_char(&painter, cell_rect, cell.ch, fg_color, geometry.cell_width) {
                continue;
            }

            let (font_name, font_size) = font_for_cell(cell);
            painter.text(
                text_paint_position(x, y, geometry.render_scale, cell),
                egui::Align2::LEFT_TOP,
                cell.ch.to_string(),
                egui::FontId::new(
                    font_size * geometry.render_scale,
                    FontFamily::Name(font_name.into()),
                ),
                fg_color,
            );
        }
    }

    let cursor_col = snap.cursor_col.min(snap.cols.saturating_sub(1));
    let cursor_cell = &snap.rows[snap.cursor_row][cursor_col];
    let cursor_width = if cursor_cell.width > 1 {
        cursor_cell.width
    } else {
        1
    };
    let cursor_x = geometry.rect.min.x + cursor_col as f32 * geometry.cell_width;
    let cursor_y = geometry.rect.min.y + snap.cursor_row as f32 * geometry.cell_height;
    painter.rect_filled(
        cursor_underline_rect(
            egui::pos2(cursor_x, cursor_y),
            geometry.cell_width,
            geometry.cell_height,
            cursor_width,
        ),
        0.0,
        egui::Color32::from_rgb(200, 200, 200),
    );
}

pub fn cursor_underline_rect(
    cell_pos: egui::Pos2,
    cell_width: f32,
    cell_height: f32,
    cursor_width: u8,
) -> egui::Rect {
    const CURSOR_UNDERLINE_HEIGHT: f32 = 2.0;
    egui::Rect::from_min_size(
        egui::pos2(
            cell_pos.x,
            cell_pos.y + cell_height - CURSOR_UNDERLINE_HEIGHT,
        ),
        egui::vec2(cell_width * cursor_width as f32, CURSOR_UNDERLINE_HEIGHT),
    )
}

pub fn text_paint_position(x: f32, y: f32, render_scale: f32, cell: &Cell) -> egui::Pos2 {
    let (x_offset, y_offset) = if cell.width > 1 {
        (CHINESE_LEFT_MARGIN, CHINESE_TOP_MARGIN)
    } else {
        (ENGLISH_LEFT_MARGIN, ENGLISH_TOP_MARGIN)
    };

    egui::pos2(x + x_offset * render_scale, y + y_offset * render_scale)
}

fn paint_terminal_edge_bleed(
    snap: &TerminalSnapshot<'_>,
    terminal_rect: egui::Rect,
    canvas_rect: egui::Rect,
    painter: &egui::Painter,
    cell_width: f32,
    cell_height: f32,
) {
    if terminal_rect.right() < canvas_rect.right() {
        for row in 0..snap.row_count {
            let cell = visible_cell_at(&snap.rows[row], snap.cols.saturating_sub(1));
            let color = cell_background_color(cell);

            let top = terminal_rect.top() + row as f32 * cell_height;
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(terminal_rect.right(), top),
                    egui::pos2(canvas_rect.right(), top + cell_height),
                ),
                0.0,
                color,
            );
        }
    }

    if terminal_rect.bottom() < canvas_rect.bottom() {
        let row = snap.row_count.saturating_sub(1);
        for col in 0..snap.cols {
            let cell = visible_cell_at(&snap.rows[row], col);
            let color = cell_background_color(cell);

            let left = terminal_rect.left() + col as f32 * cell_width;
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(left, terminal_rect.bottom()),
                    egui::pos2(left + cell_width, canvas_rect.bottom()),
                ),
                0.0,
                color,
            );
        }

        let cell = visible_cell_at(&snap.rows[row], snap.cols.saturating_sub(1));
        let color = cell_background_color(cell);
        painter.rect_filled(
            egui::Rect::from_min_max(terminal_rect.right_bottom(), canvas_rect.right_bottom()),
            0.0,
            color,
        );
    }
}

fn visible_cell_at(row: &[Cell], col: usize) -> &Cell {
    if row[col].width != 0 || col == 0 {
        &row[col]
    } else {
        &row[col - 1]
    }
}

pub fn cell_background_color(cell: &Cell) -> egui::Color32 {
    if cell.reverse {
        foreground_color(cell.fg_color, false)
    } else {
        background_color(cell.bg_color)
    }
}

pub fn cell_foreground_color(cell: &Cell) -> egui::Color32 {
    if cell.reverse {
        let bg = match cell.bg_color {
            cell::Color::Default => cell::Color::Black,
            _ => cell.bg_color,
        };
        color_to_egui(brighten(bg, cell.bold))
    } else {
        foreground_color(cell.fg_color, cell.bold)
    }
}

fn foreground_color(color: cell::Color, bold: bool) -> egui::Color32 {
    let base = match color {
        cell::Color::Default => cell::Color::White,
        _ => color,
    };
    color_to_egui(brighten(base, bold))
}

fn background_color(color: cell::Color) -> egui::Color32 {
    match color {
        cell::Color::Default => egui::Color32::BLACK,
        _ => color_to_egui(color),
    }
}

fn brighten(color: cell::Color, bold: bool) -> cell::Color {
    if !bold {
        return color;
    }
    match color {
        cell::Color::Black => cell::Color::BrightBlack,
        cell::Color::Red => cell::Color::BrightRed,
        cell::Color::Green => cell::Color::BrightGreen,
        cell::Color::Yellow => cell::Color::BrightYellow,
        cell::Color::Blue => cell::Color::BrightBlue,
        cell::Color::Magenta => cell::Color::BrightMagenta,
        cell::Color::Cyan => cell::Color::BrightCyan,
        cell::Color::White => cell::Color::BrightWhite,
        _ => color,
    }
}

pub fn terminal_render_scale(
    available_width: f32,
    available_height: f32,
    cols: usize,
    rows: usize,
) -> f32 {
    let base_width = cols as f32 * CELL_WIDTH;
    let base_height = rows as f32 * CELL_HEIGHT;
    let fit_scale = (available_width / base_width).min(available_height / base_height);
    fit_scale.clamp(MIN_ZOOM, MAX_ZOOM)
}

pub fn draw_welly_box_char(
    painter: &egui::Painter,
    rect: egui::Rect,
    ch: char,
    color: egui::Color32,
    cell_width: f32,
) -> bool {
    let stroke_width = (cell_width / 6.0).round().max(1.0);
    let half_stroke = stroke_width / 2.0;
    let center_x = rect.center().x;
    let center_y = rect.center().y;

    let horizontal = |left: f32, right: f32| {
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(left, center_y - half_stroke),
                egui::pos2(right, center_y + half_stroke),
            ),
            0.0,
            color,
        );
    };

    let vertical = |top: f32, bottom: f32| {
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(center_x - half_stroke, top),
                egui::pos2(center_x + half_stroke, bottom),
            ),
            0.0,
            color,
        );
    };

    match ch {
        '─' => horizontal(rect.left(), rect.right()),
        '│' => vertical(rect.top(), rect.bottom()),
        '┌' => {
            horizontal(center_x, rect.right());
            vertical(center_y, rect.bottom());
        }
        '┐' => {
            horizontal(rect.left(), center_x);
            vertical(center_y, rect.bottom());
        }
        '└' => {
            horizontal(center_x, rect.right());
            vertical(rect.top(), center_y);
        }
        '┘' => {
            horizontal(rect.left(), center_x);
            vertical(rect.top(), center_y);
        }
        '├' => {
            horizontal(center_x, rect.right());
            vertical(rect.top(), rect.bottom());
        }
        '┤' => {
            horizontal(rect.left(), center_x);
            vertical(rect.top(), rect.bottom());
        }
        '┬' => {
            horizontal(rect.left(), rect.right());
            vertical(center_y, rect.bottom());
        }
        '┴' => {
            horizontal(rect.left(), rect.right());
            vertical(rect.top(), center_y);
        }
        '┼' => {
            horizontal(rect.left(), rect.right());
            vertical(rect.top(), rect.bottom());
        }
        '◆' => {
            let inset_x = stroke_width;
            let inset_y = stroke_width;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(center_x, rect.top() + inset_y),
                    egui::pos2(rect.right() - inset_x, center_y),
                    egui::pos2(center_x, rect.bottom() - inset_y),
                    egui::pos2(rect.left() + inset_x, center_y),
                ],
                color,
                egui::Stroke::new(0.0, color),
            ));
        }
        '━' => horizontal(rect.left(), rect.right()),
        '┃' => vertical(rect.top(), rect.bottom()),
        '▒' => {
            let shade = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 120);
            painter.rect_filled(rect, 0.0, shade);
        }
        '█' | '◼' => {
            painter.rect_filled(rect.shrink2(egui::vec2(1.0, 1.0)), 0.0, color);
        }
        '▁'..='▇' => {
            let levels = (ch as u32 - '▁' as u32 + 1) as f32;
            let height = rect.height() * levels / 8.0;
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(rect.left(), rect.bottom() - height),
                    rect.right_bottom(),
                ),
                0.0,
                color,
            );
        }
        '▉'..='▏' => {
            let levels = 8.0 - (ch as u32 - '▉' as u32) as f32;
            let width = cell_width * levels / 8.0;
            painter.rect_filled(
                egui::Rect::from_min_size(rect.left_top(), egui::vec2(width, rect.height())),
                0.0,
                color,
            );
        }
        '▔' => {
            painter.rect_filled(
                egui::Rect::from_min_size(rect.left_top(), egui::vec2(cell_width, stroke_width)),
                0.0,
                color,
            );
        }
        '▕' => {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(rect.right() - stroke_width, rect.top()),
                    rect.right_bottom(),
                ),
                0.0,
                color,
            );
        }
        '◢' => {
            painter.add(egui::Shape::convex_polygon(
                vec![rect.left_bottom(), rect.right_bottom(), rect.right_top()],
                color,
                egui::Stroke::new(0.0, color),
            ));
        }
        '◣' => {
            painter.add(egui::Shape::convex_polygon(
                vec![rect.left_bottom(), rect.right_bottom(), rect.left_top()],
                color,
                egui::Stroke::new(0.0, color),
            ));
        }
        '◤' => {
            painter.add(egui::Shape::convex_polygon(
                vec![rect.left_top(), rect.right_top(), rect.left_bottom()],
                color,
                egui::Stroke::new(0.0, color),
            ));
        }
        '◥' => {
            painter.add(egui::Shape::convex_polygon(
                vec![rect.left_top(), rect.right_top(), rect.right_bottom()],
                color,
                egui::Stroke::new(0.0, color),
            ));
        }
        '╱' | '／' => {
            painter.line_segment(
                [rect.left_bottom(), rect.right_top()],
                egui::Stroke::new(stroke_width, color),
            );
        }
        '╲' | '﹨' | '＼' => {
            painter.line_segment(
                [rect.left_top(), rect.right_bottom()],
                egui::Stroke::new(stroke_width, color),
            );
        }
        '╳' => {
            painter.line_segment(
                [rect.left_bottom(), rect.right_top()],
                egui::Stroke::new(stroke_width, color),
            );
            painter.line_segment(
                [rect.left_top(), rect.right_bottom()],
                egui::Stroke::new(stroke_width, color),
            );
        }
        _ => return false,
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_rect_is_bottom_underline_not_full_cell() {
        let rect = cursor_underline_rect(egui::pos2(10.0, 20.0), 18.0, 35.0, 2);

        assert_eq!(rect.min, egui::pos2(10.0, 20.0 + 35.0 - 2.0));
        assert_eq!(rect.size(), egui::vec2(18.0 * 2.0, 2.0));
    }

    #[test]
    fn default_colors_reverse_to_visible_black_on_light_background() {
        let cell = crate::backend::cell::Cell {
            reverse: true,
            ..Default::default()
        };

        assert_eq!(cell_foreground_color(&cell), egui::Color32::BLACK);
        assert_eq!(
            cell_background_color(&cell),
            egui::Color32::from_rgb(229, 229, 229)
        );
    }

    #[test]
    fn terminal_render_scale_tracks_available_size() {
        let base_width = TERMINAL_COLS as f32 * CELL_WIDTH;
        let base_height = TERMINAL_ROWS as f32 * CELL_HEIGHT;

        assert!(
            (terminal_render_scale(base_width, base_height, TERMINAL_COLS, TERMINAL_ROWS) - 1.0)
                .abs()
                < f32::EPSILON
        );
        assert!(
            (terminal_render_scale(
                base_width * 2.0,
                base_height * 2.0,
                TERMINAL_COLS,
                TERMINAL_ROWS
            ) - 2.0)
                .abs()
                < f32::EPSILON
        );
        assert_eq!(
            terminal_render_scale(1.0, 1.0, TERMINAL_COLS, TERMINAL_ROWS),
            MIN_ZOOM
        );
    }
}
