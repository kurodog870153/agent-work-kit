//! Work domain operations: pure rules and derived computations.
//!
//! Nothing here touches storage, clocks, the file system or the environment.

mod codec;
mod discussion;
mod source;
mod task;

pub use codec::{decode, encode};
pub use discussion::{apply_round, new_discussion, outstanding_question, summarise};
pub use source::{new_correction, new_source};
pub use task::{
    add_attempt, append_requirement_acceptance, append_task_acceptance, complete_task,
    completion_blockers, criterion_states, new_execution, new_requirement_acceptance, new_task,
    new_task_acceptance, requirement_state, revise_task,
};
