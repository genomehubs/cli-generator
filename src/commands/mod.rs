//! CLI command implementations.
//!
//! Each subcommand lives in its own submodule.  The functions here receive
//! already-parsed clap arguments and delegate all logic to `core`.

pub mod new;
pub mod preview;
pub mod update;
pub mod validate;
