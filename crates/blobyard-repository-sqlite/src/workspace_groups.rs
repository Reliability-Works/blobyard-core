use super::{
    SqliteRepository, rows, workspace_group_members, workspace_group_mutations,
    workspace_group_queries, workspace_group_rows,
};
use blobyard_contract::{
    NewAuditEvent, RepositoryError, WorkspaceGroupCursor, WorkspaceGroupMemberCursor,
    WorkspaceGroupMemberPage, WorkspaceGroupMemberRecord, WorkspaceGroupPage, WorkspaceGroupRecord,
    WorkspaceGroupRepository, normalize_group_name,
};

impl WorkspaceGroupRepository for SqliteRepository {
    fn create_workspace_group(
        &self,
        group: &WorkspaceGroupRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.write_transaction(|transaction| {
            workspace_group_mutations::create(transaction, group, event)
        })
    }

    fn list_workspace_groups(
        &self,
        workspace_id: &str,
        cursor: Option<&WorkspaceGroupCursor>,
        limit: u32,
    ) -> Result<WorkspaceGroupPage, RepositoryError> {
        let connection = self.connection()?;
        workspace_group_queries::list_groups(&connection, workspace_id, cursor, limit)
    }

    fn rename_workspace_group(
        &self,
        workspace_id: &str,
        group_id: &str,
        name: &str,
        event: &NewAuditEvent,
    ) -> Result<WorkspaceGroupRecord, RepositoryError> {
        rows::validate_text(workspace_id)?;
        workspace_group_rows::validate_group_id(group_id)?;
        if normalize_group_name(name)? != name {
            return Err(RepositoryError::InvalidInput);
        }
        self.write_transaction(|transaction| {
            workspace_group_mutations::rename(transaction, workspace_id, group_id, name, event)
        })
    }

    fn list_workspace_group_members(
        &self,
        workspace_id: &str,
        group_id: &str,
        cursor: Option<&WorkspaceGroupMemberCursor>,
        limit: u32,
    ) -> Result<WorkspaceGroupMemberPage, RepositoryError> {
        let connection = self.connection()?;
        workspace_group_queries::list_members(&connection, workspace_id, group_id, cursor, limit)
    }

    fn add_workspace_group_member(
        &self,
        member: &WorkspaceGroupMemberRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        self.write_transaction(|transaction| {
            workspace_group_members::add(transaction, member, event)
        })
    }

    fn remove_workspace_group_member(
        &self,
        workspace_id: &str,
        group_id: &str,
        user_id: &str,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        rows::validate_text(workspace_id)?;
        self.write_transaction(|transaction| {
            workspace_group_members::remove(transaction, workspace_id, group_id, user_id, event)
        })
    }

    fn deactivate_workspace_group(
        &self,
        workspace_id: &str,
        group_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        rows::validate_text(workspace_id)?;
        self.write_transaction(|transaction| {
            workspace_group_mutations::deactivate(
                transaction,
                workspace_id,
                group_id,
                now_ms,
                event,
            )
        })
    }
}
