#![allow(dead_code)]

//! Read-only view of a `Terminal` for frontends to render.
//!
//! Borrows from the underlying [`Terminal`](super::terminal::Terminal);
//! lives only as long as the lock guard that produced it. UI-neutral —
//! no egui / gpui types in this module.

use super::cell::Cell;

pub struct TerminalSnapshot<'a> {
    pub rows: &'a [Vec<Cell>],
    pub cols: usize,
    pub row_count: usize,
    pub cursor_row: usize,
    pub cursor_col: usize,
}

impl<'a> TerminalSnapshot<'a> {
    /// Returns the cell at `(row, col)` or `None` if out of bounds.
    pub fn cell(&self, row: usize, col: usize) -> Option<&Cell> {
        self.rows.get(row)?.get(col)
    }
}
