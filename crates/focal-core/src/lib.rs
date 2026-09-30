//! Focal's editor model, independent of any UI toolkit.
//!
//! - [`analysis`] parses Markdown with `pulldown-cmark` into styles, markers
//!   and line kinds, all as byte ranges into the unchanged source.
//! - [`display`] builds each line's displayed text, hiding or replacing
//!   markers away from the caret, with a map back to source offsets.
//! - [`buffer`] holds the text and its undo history.
//! - [`editing`] computes Markdown-aware edits such as list continuation.

pub mod a11y;
pub mod analysis;
pub mod buffer;
pub mod display;
pub mod editing;
mod lines;
pub mod table;

pub use analysis::{Analysis, Bias, LinePrefix, ListMarker, PrefixLevel, analyze};
pub use buffer::{Buffer, EditKind};
pub use display::{Caret, DisplayMap, LineView, line_view, range_view};
pub use lines::LineIndex;
pub use table::TableModel;
