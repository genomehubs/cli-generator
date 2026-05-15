//! Local file parsers for positional hybrid reports.
//!
//! Parses three source formats into a common [`LocalFeatureSet`] type:
//! - [`busco`]: BUSCO `full_table.tsv` (Complete / Duplicated rows only)
//! - [`fai`]: samtools `.fai` FASTA index (sequence lengths only)
//! - [`lengths`]: explicit two-column `sequence_id<TAB>length` TSV

pub mod busco;
pub mod cat_file;
pub mod fai;
pub mod feature_set;
pub mod lengths;

pub use feature_set::{LocalFeature, LocalFeatureSet};
