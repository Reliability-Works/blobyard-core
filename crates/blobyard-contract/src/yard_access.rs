/// Persisted audience allowed to open one Yard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YardVisibility {
    /// Anyone may open the Yard without authentication.
    Public,
    /// Only the Yard owner may open the Yard.
    Owner,
    /// Only selected people and groups may open the Yard.
    Selected,
    /// Any workspace member may open the Yard.
    Workspace,
    /// Anyone holding the authenticated link may open the Yard.
    AuthenticatedLink,
    /// Any authenticated user may open the Yard.
    AnyAuthenticated,
}

impl YardVisibility {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Owner => "owner",
            Self::Selected => "selected",
            Self::Workspace => "workspace",
            Self::AuthenticatedLink => "authenticated-link",
            Self::AnyAuthenticated => "any-authenticated",
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "public" => Some(Self::Public),
            "owner" => Some(Self::Owner),
            "selected" => Some(Self::Selected),
            "workspace" => Some(Self::Workspace),
            "authenticated-link" => Some(Self::AuthenticatedLink),
            "any-authenticated" => Some(Self::AnyAuthenticated),
            _ => None,
        }
    }
}

/// Persisted kind of principal one access grant covers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YardAccessPrincipalKind {
    /// A local user account.
    User,
    /// A local group.
    Group,
    /// A guest invitation.
    GuestInvite,
    /// A capability link holder.
    Link,
}

impl YardAccessPrincipalKind {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Group => "group",
            Self::GuestInvite => "guest-invite",
            Self::Link => "link",
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Self::User),
            "group" => Some(Self::Group),
            "guest-invite" => Some(Self::GuestInvite),
            "link" => Some(Self::Link),
            _ => None,
        }
    }
}

/// Durable visibility policy for one Yard. An absent row means public.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardAccessPolicyRecord {
    /// Governed Yard identifier.
    pub yard_id: String,
    /// Persisted audience.
    pub visibility: YardVisibility,
    /// Last change as Unix milliseconds.
    pub updated_at_ms: u64,
    /// Safe label of the principal that last changed the policy.
    pub updated_by_principal: String,
}

/// Durable access grant admitting one principal into a Yard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YardAccessGrantRecord {
    /// Stable grant identifier.
    pub id: String,
    /// Governed Yard identifier.
    pub yard_id: String,
    /// Optional single-environment restriction.
    pub environment_id: Option<String>,
    /// Kind of admitted principal.
    pub principal_kind: YardAccessPrincipalKind,
    /// Stable identifier of the admitted principal.
    pub principal_id: String,
    /// Application roles the manifest declares.
    pub app_roles: Vec<String>,
    /// Persisted lifecycle state.
    pub status: crate::RevocableStatus,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Safe label of the principal that created the grant.
    pub created_by_principal: String,
    /// Optional expiry as Unix milliseconds.
    pub expires_at_ms: Option<u64>,
    /// Revocation time as Unix milliseconds when revoked.
    pub revoked_at_ms: Option<u64>,
}

/// Validated input for one new Yard access grant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewYardAccessGrant {
    /// Stable grant identifier.
    pub id: String,
    /// Governed Yard identifier.
    pub yard_id: String,
    /// Optional single-environment restriction.
    pub environment_id: Option<String>,
    /// Kind of admitted principal.
    pub principal_kind: YardAccessPrincipalKind,
    /// Stable identifier of the admitted principal.
    pub principal_id: String,
    /// Application roles the manifest declares.
    pub app_roles: Vec<String>,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Safe label of the principal creating the grant.
    pub created_by_principal: String,
    /// Optional expiry as Unix milliseconds.
    pub expires_at_ms: Option<u64>,
}

#[cfg(test)]
#[path = "yard_access_tests.rs"]
mod tests;
