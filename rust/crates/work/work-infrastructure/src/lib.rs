//! Work domain technical facts and adapters.
//!
//! Implementations live here; they execute what a feature decided and never
//! make business decisions of their own.

mod digest;
mod store;

pub use store::FsStore;
