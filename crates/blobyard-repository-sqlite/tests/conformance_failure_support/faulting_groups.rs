use super::Faulting;
use blobyard_contract::{
    NewAuditEvent, RepositoryError, WorkspaceGroupCursor, WorkspaceGroupMemberCursor,
    WorkspaceGroupMemberPage, WorkspaceGroupMemberRecord, WorkspaceGroupPage, WorkspaceGroupRecord,
    WorkspaceGroupRepository,
};

impl<T: WorkspaceGroupRepository> WorkspaceGroupRepository for Faulting<'_, T> {
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
