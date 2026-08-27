//! Pure domain logic, ported 1:1 from the former `build_site.py`.
//!
//! Nothing in this module touches the database or the network; every
//! function works on plain values so it can be unit-tested exhaustively.

pub mod collate;
pub mod export;
pub mod group;
pub mod metadata;
pub mod model;
pub mod offer_history;
pub mod text;
pub mod trend;

pub use group::group_by_strain;
pub use metadata::build_metadata;
pub use model::*;
pub use text::{calculate_thc_price, clean_text, parse_decimal, parse_percent, round2, strain_key};
pub use trend::compute_trend;
