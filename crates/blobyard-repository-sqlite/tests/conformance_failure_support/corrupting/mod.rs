mod auth;
mod inboxes;
mod lifecycle;
mod local_users;
mod metadata;
mod previews;
mod sharing;
mod transfer;
mod yard_access;
mod yard_guests;
mod yard_identity;
mod yard_oidc;
mod yard_sessions;
mod yards;

use std::sync::atomic::AtomicUsize;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Corruption {
    SchemaVersion,
    WorkspaceList,
    WorkspaceRecord,
    RenamedWorkspaceList,
    RenamedWorkspaceRecord,
    ProjectList,
    ProjectRecord,
    CompleteState,
    CompleteSize,
    CompleteChecksum,
    AbortedState,
    DownloadList,
    FirstReservationVersion,
    FirstReservationState,
    FirstCapabilityRecord,
    RenewedExpiry,
    CompletedState,
    CompletedVersion,
    SecondReservationVersion,
    LatestLength,
    LatestVersion,
    AllLength,
    AllFirstVersion,
    AllSecondVersion,
    RequestedAbortPrior,
    RequestedAbortStored,
    UploadedAbortPrior,
    UploadedAbortStored,
    DownloadVersion,
    MultipartReservation,
    MultipartAttachment,
    MultipartIssued,
    MultipartResolution,
    MultipartListing,
    MultipartCompletion,
    MultipartAbort,
    BootstrapFirstFalse,
    BootstrapSecondTrue,
    ActiveTokenRecord,
    CreatedTokenRecord,
    MonotonicTokenRecord,
    CliSessionList,
    FinalTokenListError,
    FinalTokenListMismatch,
    TokenList,
    GroupInitialList,
    GroupMemberList,
    GroupFinalCount,
    GroupMissingUser,
    GroupMissingFinal,
    GroupAuditRecord,
    GroupSuccessExtraAudit,
    GroupFailedMutationAudit,
    GroupFailedMutationSnapshot,
    LocalUserInitialList,
    LocalUserFreshAuthentication,
    LocalUserBoundaryAuthentication,
    LocalUserResetAuthentication,
    DeletionComplete,
    DeletionItems,
    DeletionReplayIncomplete,
    RetentionPolicy,
    ClearFalse,
    RetentionStatus,
    AuditPageLength,
    AuditCursor,
    AuditNextLength,
    AuditNextAction,
    ShareCreatedRecord,
    ShareList,
    ShareResolvedTarget,
    ShareIssuedTarget,
    ShareFirstRevoke,
    ShareSecondRevoke,
    ShareFinalRecord,
    ShareFinalList,
    InboxCreatedRecord,
    InboxList,
    InboxResolvedRecord,
    InboxRateAllowed,
    InboxRateLimited,
    InboxRateReset,
    InboxReservedRecord,
    InboxReservedList,
    InboxReservedCounters,
    InboxCompletedRecord,
    InboxCompletedList,
    InboxCompletedCounters,
    InboxAbortPrior,
    InboxAbortStored,
    InboxCapacityResult,
    InboxReleasedList,
    InboxReleasedCounters,
    InboxExpiryResult,
    InboxFirstRevoke,
    InboxSecondRevoke,
    InboxRevokedResolve,
    PreviewInitialList,
    PreviewObjectList,
    PreviewCreatedRecord,
    PreviewResolvedTarget,
    PreviewList,
    PreviewMissingResolution,
    PreviewExpiredResolution,
    PreviewFirstRevoke,
    PreviewSecondRevoke,
    PreviewRevokedResolution,
    YardInitialList,
    YardEnvironmentList,
    YardUnknownEnvironmentList,
    YardFixtureObjectList,
    YardReusedStart,
    YardReplacementStatus,
    YardDelayedStatus,
    YardRollbackRecord,
    YardFailureRecord,
    YardListShape,
    YardDeliveryTarget,
    YardFirstDelete,
    YardSecondDelete,
    YardFinalRecord,
    YardDeletedResolution,
    YardPhantomPolicy,
    YardPhantomGrantList,
    YardUnknownGrantList,
    YardVisibilityRecord,
    YardRestoredVisibility,
    YardPrivateDelivery,
    YardGrantRecord,
    YardScopedGrantRecord,
    YardGrantValidation,
    YardExpiredGrantList,
    YardRevokedGrantList,
    YardMissingGrantRevoke,
    YardFirstRevoke,
    YardSecondRevoke,
    YardGrantListOrder,
    YardAccessEnvironmentSeed,
    YardSessionEnvironmentSeed,
    YardSessionAdmission,
    YardSessionExchange,
    YardSessionMissingList,
    YardSessionList,
    YardDirectDeliveryTarget,
    YardDirectSessionRevoke,
    YardSessionLiveTarget,
    YardSessionPublicTarget,
    YardSessionRevocationList,
    YardSessionFirstRevoke,
    YardSessionLogoutRevoke,
    YardSessionDeactivation,
    YardGuestEnvironmentSeed,
    YardGuestCreatedRecord,
    YardGuestBoundaryScope,
    YardGuestCapacityCreateFailure,
    YardGuestCapacityOverflowAccepted,
    YardGuestIdentityRecord,
    YardOidcMemberBinding,
    YardOidcReturningBinding,
    YardOidcGuestBinding,
}

pub(crate) struct Corrupting<'a, T> {
    inner: &'a T,
    corruption: Corruption,
    inbox_list_calls: AtomicUsize,
    audit_list_calls: AtomicUsize,
    environment_list_calls: AtomicUsize,
    yard_admission_successes: AtomicUsize,
}

impl<T: blobyard_contract::WorkspaceGroupRepository> blobyard_contract::WorkspaceGroupRepository
    for Corrupting<'_, T>
{
    fn create_workspace_group(
        &self,
        group: &blobyard_contract::WorkspaceGroupRecord,
        event: &blobyard_contract::NewAuditEvent,
    ) -> Result<(), blobyard_contract::RepositoryError> {
        self.inner.create_workspace_group(group, event)
    }

    fn list_workspace_groups(
        &self,
        workspace_id: &str,
        cursor: Option<&blobyard_contract::WorkspaceGroupCursor>,
        limit: u32,
    ) -> Result<blobyard_contract::WorkspaceGroupPage, blobyard_contract::RepositoryError> {
        self.inner
            .list_workspace_groups(workspace_id, cursor, limit)
            .map(|mut page| {
                self.corrupt_group_page(&mut page);
                page
            })
    }

    fn rename_workspace_group(
        &self,
        workspace_id: &str,
        group_id: &str,
        name: &str,
        event: &blobyard_contract::NewAuditEvent,
    ) -> Result<blobyard_contract::WorkspaceGroupRecord, blobyard_contract::RepositoryError> {
        self.inner
            .rename_workspace_group(workspace_id, group_id, name, event)
    }

    fn list_workspace_group_members(
        &self,
        workspace_id: &str,
        group_id: &str,
        cursor: Option<&blobyard_contract::WorkspaceGroupMemberCursor>,
        limit: u32,
    ) -> Result<blobyard_contract::WorkspaceGroupMemberPage, blobyard_contract::RepositoryError>
    {
        self.inner
            .list_workspace_group_members(workspace_id, group_id, cursor, limit)
            .map(|mut page| {
                if matches!(self.corruption, Corruption::GroupMemberList) {
                    page.items.clear();
                }
                page
            })
    }

    fn add_workspace_group_member(
        &self,
        member: &blobyard_contract::WorkspaceGroupMemberRecord,
        event: &blobyard_contract::NewAuditEvent,
    ) -> Result<(), blobyard_contract::RepositoryError> {
        self.inner.add_workspace_group_member(member, event)
    }

    fn remove_workspace_group_member(
        &self,
        workspace_id: &str,
        group_id: &str,
        user_id: &str,
        event: &blobyard_contract::NewAuditEvent,
    ) -> Result<(), blobyard_contract::RepositoryError> {
        self.inner
            .remove_workspace_group_member(workspace_id, group_id, user_id, event)
    }

    fn deactivate_workspace_group(
        &self,
        workspace_id: &str,
        group_id: &str,
        now_ms: u64,
        event: &blobyard_contract::NewAuditEvent,
    ) -> Result<(), blobyard_contract::RepositoryError> {
        self.inner
            .deactivate_workspace_group(workspace_id, group_id, now_ms, event)
    }
}

impl<'a, T> Corrupting<'a, T> {
    fn corrupt_group_page(&self, page: &mut blobyard_contract::WorkspaceGroupPage) {
        match self.corruption {
            Corruption::GroupInitialList => {
                page.items.retain(|group| group.name != "Reviewers");
            }
            Corruption::GroupFinalCount => {
                if let Some(group) = page
                    .items
                    .iter_mut()
                    .find(|group| group.name == "Approvers")
                {
                    group.member_count = 1;
                }
            }
            Corruption::GroupMissingFinal => {
                page.items.retain(|group| group.name != "Approvers");
            }
            _ => {}
        }
    }

    pub(crate) const fn new(inner: &'a T, corruption: Corruption) -> Self {
        Self {
            inner,
            corruption,
            inbox_list_calls: AtomicUsize::new(0),
            audit_list_calls: AtomicUsize::new(0),
            environment_list_calls: AtomicUsize::new(0),
            yard_admission_successes: AtomicUsize::new(0),
        }
    }
}
