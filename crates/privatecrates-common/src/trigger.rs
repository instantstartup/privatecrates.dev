//! Which GitHub Actions events may trigger a trusted publish (SPEC §6.4), shared by the server, which refuses a
//! publish from any other, and the verifier, which rejects provenance naming any other.
//!
//! Each allowed event can only be caused by someone with Write access to the repository: pushing, publishing a
//! release, or running a workflow by hand. Others, such as `pull_request_target`, `issue_comment`, `pull_request`,
//! `workflow_run`, `schedule`, `merge_group` and `repository_dispatch`, can run a workflow for people without it.

/// The events a publishing workflow may be triggered by.
pub const PUBLISH_EVENTS: &[&str] = &["push", "release", "workflow_dispatch"];

/// Whether a workflow triggered by `event_name` may publish.
pub fn may_publish(event_name: &str) -> bool {
    PUBLISH_EVENTS.contains(&event_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_events_that_need_write_access_may_publish() {
        for event in PUBLISH_EVENTS {
            assert!(may_publish(event), "{event}");
        }
        for event in [
            "pull_request_target",
            "issue_comment",
            "pull_request",
            "workflow_run",
            "schedule",
            "merge_group",
            "repository_dispatch",
            "Push",
            "",
        ] {
            assert!(!may_publish(event), "{event}");
        }
    }
}
