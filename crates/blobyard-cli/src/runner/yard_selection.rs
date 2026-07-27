use super::{Runner, yards::select_yard};
use crate::config::validate_yard_name;
use blobyard_api_client::WebYardSummary;
use blobyard_core::{BlobyardError, Slug};

impl Runner {
    pub(super) async fn selected_named_yard(
        &self,
        name: &str,
    ) -> Result<(Slug, WebYardSummary), BlobyardError> {
        let yard = validate_yard_name(name)?;
        let (yards, _request_id) = self.all_web_yards().await?;
        let selected = select_yard(&yards, Some(yard.as_str()))?.clone();
        Ok((yard, selected))
    }
}
