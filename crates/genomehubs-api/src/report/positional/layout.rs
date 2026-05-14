//! Re-export of layout algorithms from the shared `genomehubs-query` crate.
//!
//! Logic lives in [`genomehubs_query::report::layout`].

pub use genomehubs_query::report::layout::{
    compute_offsets, order_sequences_by_median, orient_sequence, SequenceLayout,
};
