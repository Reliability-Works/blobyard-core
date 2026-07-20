use crate::Endpoint;

/// Contract ownership for an API operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationAvailability {
    /// Available in Blob Yard Cloud and self-hosted deployments.
    Core,
    /// Available only while bootstrapping a self-hosted deployment.
    SelfHostedOnly,
    /// Available only in Blob Yard Cloud.
    HostedExtension,
    /// Private server adapter operation, unavailable to public clients.
    Internal,
}

impl Endpoint {
    /// Returns where this operation is implemented.
    #[must_use]
    pub const fn availability(self) -> OperationAvailability {
        match self {
            Self::ExchangeBootstrapToken => OperationAvailability::SelfHostedOnly,
            Self::DeviceStart
            | Self::DevicePoll
            | Self::TokenRefresh
            | Self::Logout
            | Self::ListMembers
            | Self::ListInvites
            | Self::CreateInvite
            | Self::RevokeInvite
            | Self::UpdateMemberRole
            | Self::RemoveMember
            | Self::CreateOneTimeSecret
            | Self::RedeemOneTimeSecret
            | Self::CreateBillingCheckout
            | Self::CreateBillingPortal
            | Self::GetBilling
            | Self::CreateStorageCheckout
            | Self::CreateStorageUpdate
            | Self::CreateBillingSubscriptionUpdate
            | Self::RequestAccountExport
            | Self::GetAccountExport
            | Self::DownloadAccountExport
            | Self::PrepareAccountDeletion
            | Self::CompleteAccountDeletion
            | Self::GetAccountDeletion
            | Self::RetryAccountDeletion => OperationAvailability::HostedExtension,
            Self::ResolvePreview | Self::StripeWebhook => OperationAvailability::Internal,
            _ => OperationAvailability::Core,
        }
    }
}
