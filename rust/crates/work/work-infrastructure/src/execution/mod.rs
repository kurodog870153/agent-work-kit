//! Project-relative execution adapters.

mod command;
mod control_commit;
// Prepared privately; production switches together with the current command contract.
mod evidence_file;
pub mod file_transaction;
mod host_evidence;
mod project_file_metadata;
pub mod storage;
