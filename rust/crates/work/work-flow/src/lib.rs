//! Work domain use-case orchestration.
//!
//! Flow composes Feature use cases behind typed requests and responses. The
//! public contracts belong to `work-model`, the command tree to `work-cli`.

mod run;
pub use run::{
    correction_apply, correction_list, correction_preview, discussion_create, discussion_history,
    discussion_progress, discussion_round, discussion_show, execution_record, execution_show,
    requirement_acceptance_record, requirement_acceptance_show, source_create, source_show, status,
    task_acceptance_record, task_acceptance_show, task_apply, task_complete, task_preview,
    task_show, trace,
};
