//! Shared contract handling: registry construction, structural validation,
//! on-demand contract queries and scaffold derivation.
//!
//! Scope limits are deliberate: this crate holds no domain contract, performs
//! no business validation, parses no domain command input and never acts as a
//! routing or authorization authority. Each domain supplies its own resources
//! and builds its own registry.

pub mod clap_tree;
pub mod error;
pub mod model;
pub mod payload;
pub mod query;
pub mod registry;
pub mod scaffold;
pub mod validation;

pub use error::{ContractError, ContractResult};
pub use registry::{CommandEntry, DomainResources, Registry};
