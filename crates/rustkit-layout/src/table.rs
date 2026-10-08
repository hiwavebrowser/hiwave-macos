//! CSS 2.1 chapter 17 table layout (first slice).

use crate::LayoutBox;

/// `colspan` × `rowspan` of a cell, or `span` of a column (group).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableSpan {
    pub colspan: u32,
    pub rowspan: u32,
}

impl Default for TableSpan {
    fn default() -> Self {
        Self {
            colspan: 1,
            rowspan: 1,
        }
    }
}

/// CSS 2.1 §17.2.1 anonymous table objects (not yet implemented).
pub fn fixup_table_boxes(_root: &mut LayoutBox) {}
