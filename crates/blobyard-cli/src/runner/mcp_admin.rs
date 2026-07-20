use super::Runner;
use blobyard_api_client::{ApiRequest, Endpoint};
use blobyard_core::{BlobyardError, ErrorCode};
use blobyard_mcp::{AdminToolCall, Scope};
use serde_json::{Value, json};

impl Runner {
    pub(super) async fn execute_mcp_admin(
        &self,
        call: AdminToolCall,
    ) -> Result<Value, BlobyardError> {
        Ok(self.execute_mcp_admin_success(call).await?.into_data())
    }

    pub(super) async fn execute_mcp_admin_success(
        &self,
        call: AdminToolCall,
    ) -> Result<blobyard_api_client::ApiSuccess<Value>, BlobyardError> {
        require_admin_confirmation(&call)?;
        let scoped = self.mcp_scope(admin_scope(&call).clone())?;
        let request = admin_request(&scoped, call)?;
        scoped.execute_authed::<Value>(request).await
    }
}

fn require_admin_confirmation(call: &AdminToolCall) -> Result<(), BlobyardError> {
    let confirmed = match call {
        AdminToolCall::RevokeInvite { confirmed, .. }
        | AdminToolCall::UpdateMemberRole { confirmed, .. }
        | AdminToolCall::RemoveMember { confirmed, .. }
        | AdminToolCall::RevokeApiToken { confirmed, .. }
        | AdminToolCall::RevokeCiTrust { confirmed, .. }
        | AdminToolCall::RevokeCliSession { confirmed, .. } => Some(*confirmed),
        _ => None,
    };
    if confirmed == Some(false) {
        Err(BlobyardError::new(
            ErrorCode::InvalidRequest,
            "Explicit confirmation is required for this destructive operation.",
        ))
    } else {
        Ok(())
    }
}

const fn admin_scope(call: &AdminToolCall) -> &Scope {
    match call {
        AdminToolCall::ListAudit { scope, .. }
        | AdminToolCall::ListMembers { scope }
        | AdminToolCall::ListInvites { scope }
        | AdminToolCall::CreateInvite { scope, .. }
        | AdminToolCall::RevokeInvite { scope, .. }
        | AdminToolCall::UpdateMemberRole { scope, .. }
        | AdminToolCall::RemoveMember { scope, .. }
        | AdminToolCall::ListApiTokens { scope }
        | AdminToolCall::RevokeApiToken { scope, .. }
        | AdminToolCall::ListCiTrusts { scope }
        | AdminToolCall::CreateCiTrust { scope, .. }
        | AdminToolCall::RevokeCiTrust { scope, .. }
        | AdminToolCall::ListCliSessions { scope }
        | AdminToolCall::RevokeCliSession { scope, .. } => scope,
    }
}

fn admin_request(runner: &Runner, call: AdminToolCall) -> Result<ApiRequest, BlobyardError> {
    match call {
        AdminToolCall::ListAudit { cursor, .. } => Ok(ApiRequest::new(Endpoint::ListAudit)
            .with_query(workspace_query(runner, cursor.as_deref())?)),
        AdminToolCall::ListMembers { .. } => workspace_read(runner, Endpoint::ListMembers),
        AdminToolCall::ListInvites { .. } => workspace_read(runner, Endpoint::ListInvites),
        AdminToolCall::CreateInvite { email, role, .. } => workspace_write(
            runner,
            Endpoint::CreateInvite,
            &json!({ "email": email, "role": role }),
        ),
        AdminToolCall::RevokeInvite { invite_id, .. } => workspace_write(
            runner,
            Endpoint::RevokeInvite,
            &json!({ "inviteId": invite_id }),
        ),
        AdminToolCall::UpdateMemberRole { user_id, role, .. } => workspace_write(
            runner,
            Endpoint::UpdateMemberRole,
            &json!({ "targetUserId": user_id, "role": role }),
        ),
        AdminToolCall::RemoveMember { user_id, .. } => workspace_write(
            runner,
            Endpoint::RemoveMember,
            &json!({ "targetUserId": user_id }),
        ),
        AdminToolCall::ListApiTokens { .. } => Ok(ApiRequest::new(Endpoint::ListApiTokens)),
        AdminToolCall::RevokeApiToken { token_id, .. } => Ok(runner
            .mutation(Endpoint::RevokeApiToken)
            .with_json(json!({ "tokenId": token_id }))),
        AdminToolCall::ListCiTrusts { .. } => workspace_read(runner, Endpoint::ListCiTrusts),
        AdminToolCall::CreateCiTrust {
            repository,
            workflow_path,
            workflow_ref,
            allowed_ref_glob,
            allowed_actions,
            environment,
            ..
        } => create_ci_trust_request(
            runner,
            &repository,
            &workflow_path,
            &workflow_ref,
            &allowed_ref_glob,
            &allowed_actions,
            environment.as_deref(),
        ),
        AdminToolCall::RevokeCiTrust { trust_id, .. } => Ok(runner
            .mutation(Endpoint::RevokeCiTrust)
            .with_json(json!({ "trustId": trust_id }))),
        AdminToolCall::ListCliSessions { .. } => Ok(ApiRequest::new(Endpoint::ListCliSessions)),
        AdminToolCall::RevokeCliSession { session_id, .. } => Ok(runner
            .mutation(Endpoint::RevokeCliSession)
            .with_json(json!({ "sessionId": session_id }))),
    }
}

fn workspace_read(runner: &Runner, endpoint: Endpoint) -> Result<ApiRequest, BlobyardError> {
    Ok(ApiRequest::new(endpoint).with_query(workspace_query(runner, None)?))
}

fn workspace_write(
    runner: &Runner,
    endpoint: Endpoint,
    body: &Value,
) -> Result<ApiRequest, BlobyardError> {
    let mut object = body.as_object().cloned().ok_or_else(|| {
        BlobyardError::new(
            ErrorCode::InternalError,
            "The administration request is invalid.",
        )
    })?;
    object.insert("workspace".to_owned(), Value::String(workspace(runner)?));
    Ok(runner.mutation(endpoint).with_json(Value::Object(object)))
}

fn workspace_query(runner: &Runner, cursor: Option<&str>) -> Result<String, BlobyardError> {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("workspace", &workspace(runner)?);
    if let Some(value) = cursor {
        serializer.append_pair("cursor", value);
    }
    Ok(serializer.finish())
}

fn workspace(runner: &Runner) -> Result<String, BlobyardError> {
    runner
        .config
        .workspace()
        .map(ToString::to_string)
        .ok_or_else(|| BlobyardError::from_code(ErrorCode::InvalidRequest))
}

fn project(runner: &Runner) -> Option<String> {
    runner.config.project().map(ToString::to_string)
}

fn ci_trust_body(
    repository: &str,
    workflow_path: &str,
    workflow_ref: &str,
    allowed_ref_glob: &str,
    allowed_actions: &[String],
    environment: Option<&str>,
    project: Option<&str>,
) -> Value {
    let mut object = serde_json::Map::from_iter([
        ("allowedActions".to_owned(), json!(allowed_actions)),
        (
            "allowedRefGlob".to_owned(),
            Value::String(allowed_ref_glob.to_owned()),
        ),
        (
            "repository".to_owned(),
            Value::String(repository.to_owned()),
        ),
        (
            "workflowPath".to_owned(),
            Value::String(workflow_path.to_owned()),
        ),
        (
            "workflowRef".to_owned(),
            Value::String(workflow_ref.to_owned()),
        ),
    ]);
    if let Some(value) = environment {
        object.insert("environment".to_owned(), Value::String(value.to_owned()));
    }
    if let Some(value) = project {
        object.insert("project".to_owned(), Value::String(value.to_owned()));
    }
    Value::Object(object)
}

fn create_ci_trust_request(
    runner: &Runner,
    repository: &str,
    workflow_path: &str,
    workflow_ref: &str,
    allowed_ref_glob: &str,
    allowed_actions: &[String],
    environment: Option<&str>,
) -> Result<ApiRequest, BlobyardError> {
    let project = project(runner);
    let body = ci_trust_body(
        repository,
        workflow_path,
        workflow_ref,
        allowed_ref_glob,
        allowed_actions,
        environment,
        project.as_deref(),
    );
    workspace_write(runner, Endpoint::CreateCiTrust, &body)
}

#[cfg(test)]
#[path = "mcp_admin_tests.rs"]
mod tests;
