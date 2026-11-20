//! Work domain use cases, decisions and the ports they need.
//!
//! Business rules live here; external capability is reached only through a
//! port declared in this crate.

mod port;
mod task;
mod usecase;

pub use port::{Precondition, RecordStore, StoredRecord, WritePlan};
pub use usecase::{
    correction_apply, correction_preview, corrections_for, create_discussion, create_source,
    discussion_history, discussion_round, recover, show_discussion, show_progress, show_source,
    status,
};

pub use task::{
    TraceView, complete_task, record_execution, record_requirement_acceptance,
    record_task_acceptance, show_execution, show_requirement_acceptance, show_task,
    show_task_acceptance, task_apply, task_preview, trace,
};

#[cfg(test)]
mod memory;
