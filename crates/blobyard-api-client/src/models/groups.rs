use super::encoding;
use blobyard_core::Slug;
use serde::{Deserialize, Serialize};

/// Public lifecycle state for one workspace group.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GroupStatus {
    /// The group may contain members and receive Yard grants.
    Active,
    /// The group is a retained tombstone.
    Deactivated,
}

/// Stable workspace-group metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSummary {
    /// Stable group identifier.
    pub id: String,
    /// Owning workspace identifier.
    pub workspace_id: String,
    /// NFC-normalized display name.
    pub name: String,
    /// Current lifecycle state.
    pub status: GroupStatus,
    /// Creation timestamp as RFC 3339.
    pub created_at: String,
    /// Deactivation timestamp for a tombstone.
    pub deactivated_at: Option<String>,
    /// Current active membership count.
    pub member_count: u32,
}

/// Lists groups in one workspace.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListGroupsQuery {
    /// Workspace slug.
    pub workspace: Slug,
    /// Opaque next-page cursor.
    pub cursor: Option<String>,
}

impl ListGroupsQuery {
    /// Encodes the group-list query.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::query(&[
            ("workspace", Some(self.workspace.to_string())),
            ("cursor", self.cursor),
        ])
    }
}

/// One newest-first page of groups.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListGroupsResponse {
    /// Page items.
    pub items: Vec<GroupSummary>,
    /// Opaque cursor for the next page.
    pub next_cursor: Option<String>,
}

/// Creates one empty group.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateGroupRequest {
    /// Workspace slug.
    pub workspace: Slug,
    /// Human-readable group name.
    pub name: String,
}

/// A successful group create or rename.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupResponse {
    /// Created or updated group.
    pub group: GroupSummary,
}

/// Renames one active group.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenameGroupRequest {
    /// Stable group identifier.
    pub group_id: String,
    /// Replacement display name.
    pub name: String,
}

/// Lists current members of one active group.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListGroupMembersQuery {
    /// Stable group identifier.
    pub group_id: String,
    /// Opaque next-page cursor.
    pub cursor: Option<String>,
}

impl ListGroupMembersQuery {
    /// Encodes the group-member query.
    #[must_use]
    pub fn into_query(self) -> String {
        encoding::query(&[("groupId", Some(self.group_id)), ("cursor", self.cursor)])
    }
}

/// One newest-first page of group member identifiers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListGroupMembersResponse {
    /// Current user identifiers.
    pub items: Vec<String>,
    /// Opaque cursor for the next page.
    pub next_cursor: Option<String>,
}

/// Adds or removes one group membership.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupMemberRequest {
    /// Stable group identifier.
    pub group_id: String,
    /// Stable local-user identifier.
    pub user_id: String,
}

/// Deactivates one active group.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeactivateGroupRequest {
    /// Stable group identifier.
    pub group_id: String,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

    use super::*;

    #[test]
    fn group_queries_encode_optional_cursors() {
        let workspace = Slug::new("main").expect("workspace slug");
        assert_eq!(
            ListGroupsQuery {
                workspace: workspace.clone(),
                cursor: Some("group-next".to_owned()),
            }
            .into_query(),
            "workspace=main&cursor=group-next"
        );
        assert_eq!(
            ListGroupsQuery {
                workspace,
                cursor: None,
            }
            .into_query(),
            "workspace=main"
        );
        assert_eq!(
            ListGroupMembersQuery {
                group_id: "group_fixture".to_owned(),
                cursor: Some("member-next".to_owned()),
            }
            .into_query(),
            "groupId=group_fixture&cursor=member-next"
        );
        assert_eq!(
            ListGroupMembersQuery {
                group_id: "group_fixture".to_owned(),
                cursor: None,
            }
            .into_query(),
            "groupId=group_fixture"
        );
    }
}
