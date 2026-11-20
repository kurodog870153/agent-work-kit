//! Skill domain use cases and the ports they require.
//!
//! Feature decides what to load, validate and reuse; `skill-infrastructure`
//! implements the `SourceStore` port against the file system.

mod discover;
mod loader;
mod port;
mod resolve;
mod validate;

pub use discover::{Child, Discovery, discover};
pub use loader::Loaded;
pub use port::{DirEntry, SourceContent, SourceStore};
pub use resolve::{
    Delta, ReferenceSelection, Request, Resolution, Selection, plan_switch, resolve,
};
pub use validate::{Report, validate_all};

#[cfg(test)]
mod memory;
