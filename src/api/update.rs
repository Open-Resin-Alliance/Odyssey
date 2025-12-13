use itertools::Itertools;
use poem::Result;
use poem_openapi::{param::Query, payload::Json, OpenApi};
use tokio::task::spawn_blocking;
use tracing::instrument;

use crate::{api_objects::ReleaseVersion, error::OdysseyError, updates};

#[derive(Debug)]
pub struct UpdateApi;

#[OpenApi(prefix_path = "/update")]
impl UpdateApi {
    #[instrument(ret)]
    #[oai(path = "/releases", method = "get")]
    async fn get_releases(&self) -> Result<Json<Vec<ReleaseVersion>>> {
        let releases_result = spawn_blocking(updates::get_releases)
            .await
            .map_err(OdysseyError::from)?;

        Ok(Json(
            releases_result?
                .iter()
                .map(|rel| ReleaseVersion {
                    name: rel.name.clone(),
                    version: rel.version.clone(),
                    date: rel.date.clone(),
                    body: rel.body.clone(),
                })
                .collect_vec(),
        ))
    }

    #[instrument(ret)]
    #[oai(path = "/", method = "post")]
    async fn update(&self, Query(release): Query<String>) -> Result<()> {
        Ok(spawn_blocking(|| updates::update(release))
            .await
            .map_err(OdysseyError::from)??)
    }
}
