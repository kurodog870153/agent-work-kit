//! Classify Tooling operations for Host coordination.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveOperation {
    /// Canonical target supplied after Host target validation.
    pub target: PathBuf,
    pub access: Access,
    pub session: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coordination {
    ParallelRead,
    Independent,
    Serialize,
    RecheckSession,
    InspectUnknown,
}

pub fn coordinate(current: &ActiveOperation, next: &ActiveOperation) -> Coordination {
    if current.access == Access::Unknown || next.access == Access::Unknown {
        return Coordination::InspectUnknown;
    }
    if current.session != next.session && (current.session.is_some() || next.session.is_some()) {
        return Coordination::RecheckSession;
    }
    if current.session.is_some() {
        return Coordination::Serialize;
    }
    if current.target != next.target {
        return Coordination::Independent;
    }
    if current.access == Access::Read && next.access == Access::Read {
        Coordination::ParallelRead
    } else {
        Coordination::Serialize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallel_reads_and_conflicting_writes_use_actual_target() {
        let target = PathBuf::from("/project/source.rs");
        let read = ActiveOperation {
            target: target.clone(),
            access: Access::Read,
            session: None,
        };
        let write = ActiveOperation {
            target,
            access: Access::Write,
            session: None,
        };
        assert_eq!(coordinate(&read, &read), Coordination::ParallelRead);
        assert_eq!(coordinate(&read, &write), Coordination::Serialize);
        assert_eq!(coordinate(&write, &write), Coordination::Serialize);
        let other = ActiveOperation {
            target: PathBuf::from("/project/other.rs"),
            ..write.clone()
        };
        assert_eq!(coordinate(&write, &other), Coordination::Independent);
        let unknown = ActiveOperation {
            access: Access::Unknown,
            ..read.clone()
        };
        assert_eq!(coordinate(&unknown, &read), Coordination::InspectUnknown);
    }

    #[test]
    fn shared_session_serializes_and_switch_requires_recheck() {
        let a = ActiveOperation {
            target: PathBuf::from("/project/source.rs"),
            access: Access::Read,
            session: Some("session-a".into()),
        };
        let same = ActiveOperation {
            target: PathBuf::from("/project/other.rs"),
            ..a.clone()
        };
        assert_eq!(coordinate(&a, &same), Coordination::Serialize);
        let switched = ActiveOperation {
            session: Some("session-b".into()),
            ..a.clone()
        };
        assert_eq!(coordinate(&a, &switched), Coordination::RecheckSession);
    }
}
