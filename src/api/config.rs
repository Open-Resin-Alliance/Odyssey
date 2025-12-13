use std::sync::Arc;

use optional_struct::Applicable;
use poem::{web::Data, Result};
use poem_openapi::{payload::Json, OpenApi};
use tracing::instrument;

use crate::configuration::{Configuration, UpdateConfiguration};

#[derive(Debug)]
pub struct ConfigApi;

#[OpenApi(prefix_path = "/config")]
impl ConfigApi {
    #[instrument(ret, skip(full_config))]
    #[oai(path = "/", method = "get")]
    async fn get_config(
        &self,
        Data(full_config): Data<&Arc<Configuration>>,
    ) -> Json<Configuration> {
        Json(full_config.as_ref().clone())
    }

    #[instrument(ret, skip(full_config))]
    #[oai(path = "/", method = "patch")]
    async fn patch_config(
        &self,
        Data(full_config): Data<&Arc<Configuration>>,
        Json(patch_config): Json<UpdateConfiguration>,
    ) -> Result<Json<Configuration>> {
        let ammend_config = patch_config.build(full_config.as_ref().clone());
        Configuration::overwrite_file(&ammend_config)?;

        Ok(Json(ammend_config))
    }
}
