use super::FaultingRepository;
use blobyard_contract::{
    NewAuditEvent, RepositoryError, WorkspaceGroupCursor, WorkspaceGroupMemberCursor,
    WorkspaceGroupMemberPage, WorkspaceGroupMemberRecord, WorkspaceGroupPage, WorkspaceGroupRecord,
    WorkspaceGroupRepository,
};

impl WorkspaceGroupRepository for FaultingRepository {
    fn create_workspace_group(
        &self,
        group: &WorkspaceGroupRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.create_workspace_group(group, event)
    }

    fn list_workspace_groups(
        &self,
        workspace_id: &str,
        cursor: Option<&WorkspaceGroupCursor>,
        limit: u32,
    ) -> Result<WorkspaceGroupPage, RepositoryError> {
        self.check()?;
        self.inner
            .list_workspace_groups(workspace_id, cursor, limit)
    }

    fn rename_workspace_group(
        &self,
        workspace_id: &str,
        group_id: &str,
        name: &str,
        event: &NewAuditEvent,
    ) -> Result<WorkspaceGroupRecord, RepositoryError> {
        self.check()?;
        self.inner
            .rename_workspace_group(workspace_id, group_id, name, event)
    }

    fn list_workspace_group_members(
        &self,
        workspace_id: &str,
        group_id: &str,
        cursor: Option<&WorkspaceGroupMemberCursor>,
        limit: u32,
    ) -> Result<WorkspaceGroupMemberPage, RepositoryError> {
        self.check()?;
        self.inner
            .list_workspace_group_members(workspace_id, group_id, cursor, limit)
    }

    fn add_workspace_group_member(
        &self,
        member: &WorkspaceGroupMemberRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner.add_workspace_group_member(member, event)
    }

    fn remove_workspace_group_member(
        &self,
        workspace_id: &str,
        group_id: &str,
        user_id: &str,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner
            .remove_workspace_group_member(workspace_id, group_id, user_id, event)
    }

    fn deactivate_workspace_group(
        &self,
        workspace_id: &str,
        group_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.check()?;
        self.inner
            .deactivate_workspace_group(workspace_id, group_id, now_ms, event)
    }
}

#[test]
fn faulting_repository_forwards_the_complete_group_contract() {
    let (_temporary, inner) = super::conforming_repository();
    let repository = FaultingRepository::new(inner, usize::MAX);
    blobyard_testkit::local_user_conformance(&repository, "workspace_fixture")
        .expect("local user conformance");
    blobyard_testkit::group_conformance(&repository, "workspace_fixture")
        .expect("group conformance");
}

#[test]
fn every_group_operation_fails_at_the_repository_seam() {
    let (_temporary, inner) = super::conforming_repository();
    let group = blobyard_contract::WorkspaceGroupRecord {
        id: "group_00000000000000000000000000000001".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        name: "Reviewers".to_owned(),
        status: blobyard_contract::WorkspaceGroupStatus::Active,
        member_count: 0,
        created_at_ms: 1,
        deactivated_at_ms: None,
    };
    let event = blobyard_testkit::group_event(
        "audit_group_fault",
        "group.created",
        &group,
        1,
        [(
            "name",
            blobyard_contract::AuditValue::String(group.name.clone()),
        )],
    );
    let member = WorkspaceGroupMemberRecord {
        group_id: group.id.clone(),
        workspace_id: group.workspace_id.clone(),
        user_id: "user_first".to_owned(),
        added_at_ms: 2,
    };
    let faulted = || FaultingRepository::new(inner.clone(), 0);
    assert_eq!(
        faulted().create_workspace_group(&group, &event),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().list_workspace_groups(&group.workspace_id, None, 50),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().rename_workspace_group(&group.workspace_id, &group.id, "Approvers", &event,),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().list_workspace_group_members(&group.workspace_id, &group.id, None, 50),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().add_workspace_group_member(&member, &event),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().remove_workspace_group_member(
            &group.workspace_id,
            &group.id,
            &member.user_id,
            &event,
        ),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        faulted().deactivate_workspace_group(&group.workspace_id, &group.id, 3, &event),
        Err(RepositoryError::Unavailable)
    );
}
