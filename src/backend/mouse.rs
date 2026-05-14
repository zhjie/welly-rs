#![allow(dead_code)]

//! Mouse → byte-stream helpers for Welly-style BBS navigation.

use super::input::{GridPoint, WheelDir};
use super::terminal::Terminal;

const TERMINAL_ROWS: usize = 24;

pub fn bytes_for_wheel(dir: WheelDir) -> Vec<u8> {
    match dir {
        WheelDir::Up => b"\x1b[A".to_vec(),
        WheelDir::Down => b"\x1b[B".to_vec(),
        WheelDir::Left => b"\x1b[D".to_vec(),
        WheelDir::Right => b"\x1b[C".to_vec(),
    }
}

pub fn bytes_for_entry_click(cursor_row: usize, target_row: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    if target_row > cursor_row {
        for _ in cursor_row..target_row {
            bytes.extend_from_slice(b"\x1b[B");
        }
    } else {
        for _ in target_row..cursor_row {
            bytes.extend_from_slice(b"\x1b[A");
        }
    }
    bytes.push(b'\r');
    bytes
}

pub fn bytes_for_background_navigation(point: GridPoint) -> Option<Vec<u8>> {
    if point.col == 0 && (3..TERMINAL_ROWS.saturating_sub(1)).contains(&point.row) {
        return Some(b"\x1b[D".to_vec());
    }

    if point.col >= 20 {
        if point.row < TERMINAL_ROWS / 2 {
            return Some(b"\x1b[5~".to_vec());
        }
        return Some(b"\x1b[6~".to_vec());
    }

    None
}

/// True if `point` lies on a visible BBS entry row (rows 3..bottom-1,
/// columns covering the visible non-blank range, padded to at least the
/// first 30 columns).
pub fn is_entry_click_point(term: &Terminal, point: GridPoint) -> bool {
    if !(3..term.rows.saturating_sub(1)).contains(&point.row) || point.col < 2 {
        return false;
    }

    let row = &term.grid[point.row];
    let Some(start) = row
        .iter()
        .enumerate()
        .skip(2)
        .find_map(|(col, cell)| (cell.width != 0 && cell.ch != ' ').then_some(col))
    else {
        return false;
    };

    let Some(end) = row.iter().enumerate().rev().find_map(|(col, cell)| {
        (cell.width != 0 && cell.ch != ' ' && cell.ch != '\0').then_some(col)
    }) else {
        return false;
    };

    let click_end = end
        .max(start.saturating_add(29))
        .min(term.cols.saturating_sub(1));
    (start..=click_end).contains(&point.col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_directions_match_welly_arrows() {
        assert_eq!(bytes_for_wheel(WheelDir::Up), b"\x1b[A".to_vec());
        assert_eq!(bytes_for_wheel(WheelDir::Down), b"\x1b[B".to_vec());
        assert_eq!(bytes_for_wheel(WheelDir::Left), b"\x1b[D".to_vec());
        assert_eq!(bytes_for_wheel(WheelDir::Right), b"\x1b[C".to_vec());
    }

    #[test]
    fn entry_click_moves_cursor_to_row_and_enters() {
        assert_eq!(
            bytes_for_entry_click(3, 6),
            b"\x1b[B\x1b[B\x1b[B\r".to_vec()
        );
        assert_eq!(
            bytes_for_entry_click(6, 3),
            b"\x1b[A\x1b[A\x1b[A\r".to_vec()
        );
    }

    #[test]
    fn background_areas_map_to_welly_navigation_keys() {
        assert_eq!(
            bytes_for_background_navigation(GridPoint { row: 8, col: 0 }),
            Some(b"\x1b[D".to_vec())
        );
        assert_eq!(
            bytes_for_background_navigation(GridPoint { row: 4, col: 30 }),
            Some(b"\x1b[5~".to_vec())
        );
        assert_eq!(
            bytes_for_background_navigation(GridPoint { row: 18, col: 30 }),
            Some(b"\x1b[6~".to_vec())
        );
        assert_eq!(
            bytes_for_background_navigation(GridPoint { row: 8, col: 10 }),
            None
        );
    }

    #[test]
    fn entry_click_point_uses_visible_text_range() {
        let mut terminal = Terminal::new(24, 80);
        terminal.set_cursor(5, 12);
        for ch in "Re: title".chars() {
            terminal.put_char(ch);
        }

        assert!(is_entry_click_point(
            &terminal,
            GridPoint { row: 5, col: 12 }
        ));
        assert!(is_entry_click_point(
            &terminal,
            GridPoint { row: 5, col: 38 }
        ));
        assert!(!is_entry_click_point(
            &terminal,
            GridPoint { row: 5, col: 60 }
        ));
        assert!(!is_entry_click_point(
            &terminal,
            GridPoint { row: 2, col: 12 }
        ));
    }
}
