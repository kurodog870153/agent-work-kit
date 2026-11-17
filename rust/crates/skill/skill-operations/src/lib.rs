//! Skill domain operations: pure rules over text and paths.

pub mod document;
pub mod path;

pub use document::{decode, parse_document};
