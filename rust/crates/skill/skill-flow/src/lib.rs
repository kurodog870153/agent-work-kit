//! Skill domain use-case orchestration.
//!
//! Flow composes Feature use cases behind typed requests and responses. The
//! public contracts and data shapes belong to `skill-model`, the command tree
//! to `skill-cli`.

mod run;

pub use run::{discover, into_request, resolve, switch, validate};
