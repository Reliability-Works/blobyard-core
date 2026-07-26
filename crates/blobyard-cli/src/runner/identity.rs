use super::{Runner, command_result};
use blobyard_api_client::{ApiRequest, Endpoint};
use blobyard_core::BlobyardError;

impl Runner {
    pub(super) async fn whoami(&self) -> Result<crate::CommandResult, BlobyardError> {
        let success = self
            .execute_authed::<blobyard_api_client::WhoAmIResponse>(ApiRequest::new(
                Endpoint::WhoAmI,
            ))
            .await?;
        let human = whoami_human(success.data());
        command_result(success.data(), human, success.request_id())
    }
}

fn whoami_human(identity: &blobyard_api_client::WhoAmIResponse) -> String {
    let principal = identity.email.as_ref().map_or_else(
        || format!("{} ({})", identity.display_name, identity.principal_id),
        |email| {
            format!(
                "{} <{email}> ({})",
                identity.display_name, identity.principal_id
            )
        },
    );
    format!(
        "{principal}\nWorkspace: {} ({})\nScopes: {}",
        identity.default_workspace.name,
        identity.default_workspace.slug,
        identity.scopes.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::whoami_human;
    use blobyard_api_client::{PrincipalType, WhoAmIDefaultWorkspace, WhoAmIResponse};

    #[test]
    fn identity_copy_omits_email_punctuation_for_ci() {
        let workspace = WhoAmIDefaultWorkspace {
            id: "workspace_1".into(),
            name: "Builds".into(),
            slug: "builds".into(),
        };
        let cli = WhoAmIResponse {
            default_workspace: workspace.clone(),
            display_name: "Developer".into(),
            email: Some("developer@example.com".into()),
            principal_id: "user_1".into(),
            principal_type: PrincipalType::Cli,
            scopes: vec!["object:read".into()],
        };
        assert!(whoami_human(&cli).contains("Developer <developer@example.com> (user_1)"));
        let ci = WhoAmIResponse {
            default_workspace: workspace,
            display_name: "GitHub acme/artifacts".into(),
            email: None,
            principal_id: "machine_1".into(),
            principal_type: PrincipalType::Ci,
            scopes: vec!["upload".into()],
        };
        let output = whoami_human(&ci);
        assert!(output.starts_with("GitHub acme/artifacts (machine_1)"));
        assert!(!output.contains('<'));
    }
}
