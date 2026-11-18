//! CLI command implementations.
//!
//! Each subcommand lives in its own submodule.  The functions here receive
//! already-parsed clap arguments and delegate all logic to `core`.

pub mod check;
pub mod count;
pub mod describe;
pub mod local_report;
pub mod lookup;
pub mod new;
pub mod preview;
pub mod search;
pub mod update;
pub mod validate;
