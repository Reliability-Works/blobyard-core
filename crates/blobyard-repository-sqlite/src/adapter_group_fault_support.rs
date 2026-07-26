use crate::SqliteRepository;
use blobyard_contract::WebYardRepository;

pub(super) fn group_count(repository: &SqliteRepository, group_id: &str) -> i64 {
    repository
        .test_connection()
        .expect("connection")
        .query_row(
            "SELECT COUNT(*) FROM workspace_groups WHERE id = ?1",
            [group_id],
            |row| row.get(0),
        )
        .expect("group count")
}

pub(super) fn group_count_and_members(repository: &SqliteRepository, group_id: &str) -> (i64, i64) {
    repository
        .test_connection()
        .expect("connection")
        .query_row(
            "SELECT g.member_count, COUNT(m.user_id) FROM workspace_groups g
             LEFT JOIN workspace_group_members m ON m.group_id = g.id
             WHERE g.id = ?1 GROUP BY g.id",
            [group_id],
            |row| Ok((row.get_unwrap(0), row.get_unwrap(1))),
        )
        .expect("group membership state")
}

pub(super) fn group_state(
    repository: &SqliteRepository,
    group_id: &str,
) -> (String, String, i64, Option<i64>) {
    repository
        .test_connection()
        .expect("connection")
        .query_row(
            "SELECT name, status, member_count, deactivated_at_ms
             FROM workspace_groups WHERE id = ?1",
            [group_id],
            |row| {
                Ok((
                    row.get_unwrap(0),
                    row.get_unwrap(1),
                    row.get_unwrap(2),
                    row.get_unwrap(3),
                ))
            },
        )
        .expect("group state")
}

pub(super) fn audit_exists(repository: &SqliteRepository, audit_id: &str) -> bool {
    repository
        .test_connection()
        .expect("connection")
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM audit_events WHERE id = ?1)",
            [audit_id],
            |row| row.get(0),
        )
        .expect("audit state")
}

pub(super) fn seed_yard(repository: &SqliteRepository) -> blobyard_contract::NewWebYard {
    let yard = blobyard_contract::NewWebYard {
        id: "yard_group_fault".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: blobyard_core::Slug::new("fault-yard").expect("slug"),
        host_label: "fault-yard-123456789-fixture".to_owned(),
        created_at_ms: 41,
    };
    repository
        .start_yard_deploy(
            &yard,
            &blobyard_contract::NewYardDeploy {
                id: "deploy_group_fault".to_owned(),
                yard_id: yard.id.clone(),
                workspace_id: yard.workspace_id.clone(),
                project_id: yard.project_id.clone(),
                client_deploy_id: "clientdeploy00000051".to_owned(),
                manifest_root: format!(".blobyard-yard/{}/clientdeploy00000051/", yard.id),
                deployment_host_label: "fault-yard-0123456789-fixture".to_owned(),
                spa: true,
                clean_urls: true,
                created_at_ms: 41,
            },
            &blobyard_testkit::yard_event("yard.created", "web_yard", "yardId", &yard.id, 41),
        )
        .expect("yard");
    yard
}
