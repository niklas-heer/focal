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
pub mod blocks;
pub mod buffer;
pub mod callouts;
pub mod display;
pub mod editing;
pub mod encoding;
pub mod export;
pub mod find;
pub mod fuzzy;
pub mod helix;
pub mod html_inline;
pub mod keys;
mod lines;
pub mod links;
pub mod modal;
pub mod outline;
pub mod shadow;
pub mod table;
pub mod texmath;
pub mod text_stats;
pub mod vim;

pub use analysis::{Analysis, Bias, LinePrefix, ListMarker, PrefixLevel, analyze};
pub use buffer::{Buffer, EditKind};
pub use display::{Caret, DisplayMap, LineView, line_view, range_view};
pub use lines::LineIndex;
pub use table::TableModel;
