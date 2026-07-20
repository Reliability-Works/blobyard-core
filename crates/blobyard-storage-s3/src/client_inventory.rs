use crate::client::S3Client;
use crate::transport::RequestBody;
use blobyard_contract::StorageError;
use http::{HeaderMap, Method};

const XML_BODY_LIMIT: usize = 4 * 1024 * 1024;

impl S3Client {
    pub(crate) async fn list_objects(
        &self,
        prefix: Option<&str>,
        continuation: Option<&str>,
    ) -> Result<crate::xml::ListedPage, StorageError> {
        let mut query = vec![("list-type", "2")];
        if let Some(prefix) = prefix {
            query.push(("prefix", prefix));
        }
        if let Some(continuation) = continuation {
            query.push(("continuation-token", continuation));
        }
        let response = self
            .send(
                Method::GET,
                None,
                &query,
                HeaderMap::new(),
                RequestBody::Empty,
                Self::empty_hash(),
            )
            .await?;
        let body = response.collect(XML_BODY_LIMIT).await?;
        crate::xml::parse_list(&body)
    }
}
