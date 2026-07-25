use crate::catalog_contracts::{add, boolean, scope_properties, string, title, tool_schema};
use serde_json::{Map, Value, json};

#[derive(Clone, Copy)]
enum Kind {
    List,
    Create,
    Rename,
    ListMembers,
    AddMember,
    RemoveMember,
    Deactivate,
}

const KINDS: [Kind; 7] = [
    Kind::List,
    Kind::Create,
    Kind::Rename,
    Kind::ListMembers,
    Kind::AddMember,
    Kind::RemoveMember,
    Kind::Deactivate,
];

pub(super) fn tools() -> impl Iterator<Item = Value> {
    KINDS.into_iter().map(tool)
}

fn tool(kind: Kind) -> Value {
    let mut properties = scope_properties();
    let (description, mut required) = contract(kind, &mut properties);
    if destructive(kind) {
        add(
            &mut properties,
            "confirm",
            boolean("Must be true to confirm this destructive operation."),
        );
        required.push("confirm");
    }
    let contract_annotations = annotations(kind);
    tool_schema(
        kind.name(),
        description,
        &properties,
        &required,
        &contract_annotations,
    )
}

fn contract(kind: Kind, properties: &mut Map<String, Value>) -> (&'static str, Vec<&'static str>) {
    match kind {
        Kind::List => {
            add(properties, "cursor", string("Optional opaque page cursor."));
            ("List workspace groups.", vec!["workspace"])
        }
        Kind::Create => {
            add(properties, "name", string("Human-readable group name."));
            (
                "Create an empty workspace group.",
                vec!["workspace", "name"],
            )
        }
        Kind::Rename => {
            group_id(properties);
            add(properties, "name", string("Replacement group name."));
            ("Rename an active group.", vec!["group_id", "name"])
        }
        Kind::ListMembers => {
            group_id(properties);
            add(properties, "cursor", string("Optional opaque page cursor."));
            ("List current group members.", vec!["group_id"])
        }
        Kind::AddMember | Kind::RemoveMember => {
            group_id(properties);
            add(
                properties,
                "user_id",
                string("Stable local-user identifier."),
            );
            let description = if matches!(kind, Kind::AddMember) {
                "Add one active local user to a group."
            } else {
                "Remove one current group member."
            };
            (description, vec!["group_id", "user_id"])
        }
        Kind::Deactivate => {
            group_id(properties);
            (
                "Deactivate a group and revoke its active grants.",
                vec!["group_id"],
            )
        }
    }
}

fn group_id(properties: &mut Map<String, Value>) {
    add(properties, "group_id", string("Stable group identifier."));
}

fn annotations(kind: Kind) -> Value {
    let read_only = matches!(kind, Kind::List | Kind::ListMembers);
    let destructive = destructive(kind);
    json!({
        "title": title(kind.name()),
        "readOnlyHint": read_only,
        "destructiveHint": destructive,
        "idempotentHint": read_only,
        "openWorldHint": false
    })
}

const fn destructive(kind: Kind) -> bool {
    matches!(kind, Kind::RemoveMember | Kind::Deactivate)
}

impl Kind {
    const fn name(self) -> &'static str {
        match self {
            Self::List => "list_groups",
            Self::Create => "create_group",
            Self::Rename => "rename_group",
            Self::ListMembers => "list_group_members",
            Self::AddMember => "add_group_member",
            Self::RemoveMember => "remove_group_member",
            Self::Deactivate => "deactivate_group",
        }
    }
}
