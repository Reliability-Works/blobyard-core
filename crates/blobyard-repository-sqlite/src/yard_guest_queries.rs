use super::{map_error, rows, yard_guest_rows};
use blobyard_contract::{
    RepositoryError, YARD_GUEST_INVITE_PAGE_SIZE, YardGuestInviteCursor, YardGuestInvitePage,
    YardGuestInviteRecord,
};
use rusqlite::{Connection, OptionalExtension, Statement, params};

pub(super) fn list(
    connection: &Connection,
    yard_id: &str,
    cursor: Option<&YardGuestInviteCursor>,
    limit: usize,
) -> Result<YardGuestInvitePage, RepositoryError> {
    rows::validate_text(yard_id)?;
    if limit == 0 || limit > YARD_GUEST_INVITE_PAGE_SIZE {
        return Err(RepositoryError::InvalidInput);
    }
    let cursor_time = cursor
        .map(|value| super::auth_validation::sql_time(value.created_at_ms))
        .transpose()?;
    if let Some(position) = cursor {
        rows::validate_text(&position.id)?;
    }
    let mut statement = connection
        .prepare(&format!(
            "SELECT {} FROM yard_guest_invitations i
             JOIN yard_access_grants g ON g.id = i.grant_id AND g.yard_id = i.yard_id
             WHERE i.yard_id = ?1
               AND (?2 IS NULL OR i.created_at_ms < ?2 OR (i.created_at_ms = ?2 AND i.id < ?3))
             ORDER BY i.created_at_ms DESC, i.id DESC LIMIT ?4",
            yard_guest_rows::INVITATION_COLUMNS
        ))
        .map_err(map_error)?;
    #[expect(
        clippy::cast_possible_wrap,
        reason = "limit was validated as no greater than the 50-item page size"
    )]
    let query_limit = (limit + 1) as i64;
    let mut items = query(
        &mut statement,
        yard_id,
        cursor_time,
        cursor.map(|value| value.id.as_str()),
        query_limit,
    )?;
    let next_cursor = if items.len() > limit {
        let position = items.get(limit - 1).map(yard_guest_rows::cursor);
        items.truncate(limit);
        position
    } else {
        None
    };
    Ok(YardGuestInvitePage { items, next_cursor })
}

fn query(
    statement: &mut Statement<'_>,
    yard_id: &str,
    cursor_time: Option<i64>,
    cursor_id: Option<&str>,
    limit: i64,
) -> Result<Vec<YardGuestInviteRecord>, RepositoryError> {
    statement
        .query_map(
            params![yard_id, cursor_time, cursor_id, limit],
            yard_guest_rows::invitation,
        )
        .map_err(map_error)
        .and_then(super::collect)
}

pub(super) fn by_id(
    connection: &Connection,
    invitation_id: &str,
) -> Result<Option<YardGuestInviteRecord>, RepositoryError> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM yard_guest_invitations i
                 JOIN yard_access_grants g ON g.id = i.grant_id AND g.yard_id = i.yard_id
                 WHERE i.id = ?1",
                yard_guest_rows::INVITATION_COLUMNS
            ),
            [invitation_id],
            yard_guest_rows::invitation,
        )
        .optional()
        .map_err(map_error)
}

pub(super) fn pending_by_hash(
    connection: &Connection,
    token_hash: &str,
    now_ms: i64,
) -> Result<YardGuestInviteRecord, RepositoryError> {
    super::auth_validation::validate_hash(token_hash)?;
    let mut statement = connection
        .prepare(&format!(
            "SELECT {} FROM yard_guest_invitations i
             JOIN yard_access_grants g ON g.id = i.grant_id AND g.yard_id = i.yard_id
             WHERE i.token_hash = ?1 AND i.status = 'pending'
               AND i.expires_at_ms > ?2
               AND g.status = 'active' AND g.revoked_at_ms IS NULL
               AND g.expires_at_ms = i.expires_at_ms
             LIMIT 2",
            yard_guest_rows::INVITATION_COLUMNS
        ))
        .map_err(map_error)?;
    let mut matches = statement
        .query_map(params![token_hash, now_ms], yard_guest_rows::invitation)
        .map_err(map_error)
        .and_then(super::collect)?;
    if matches.len() == 1 {
        matches.pop().ok_or(RepositoryError::Unavailable)
    } else {
        Err(RepositoryError::NotFound)
    }
}
