use std::sync::Arc;

use optional_struct::Applicable;
use poem::{web::Data, Result};
use poem_openapi::{param::Path, payload::Json, types::ToJSON, OpenApi};
use serde_json::Value;
use tracing::instrument;

use crate::{
    configuration::{Configuration, UpdateConfiguration},
    error::OdysseyError,
};

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

    #[oai(path = "/:category", method = "get")]
    async fn get_config_category(
        &self,
        Path(category): Path<String>,
        Data(full_config): Data<&Arc<Configuration>>,
    ) -> Result<Json<Value>> {
        Ok(Json(
            full_config
                .to_json()
                .as_ref()
                .and_then(Value::as_object)
                .and_then(|obj| obj.get(&category))
                .ok_or(OdysseyError::configuration_error(
                    format!("No such configuration category {category}").into(),
                    404,
                ))?
                .clone(),
        ))
    }

    #[oai(path = "/:category/:field", method = "get")]
    async fn get_config_field(
        &self,
        Path(category): Path<String>,
        Path(field): Path<String>,
        Data(full_config): Data<&Arc<Configuration>>,
    ) -> Result<Json<Value>> {
        Ok(Json(
            full_config
                .to_json()
                .as_ref()
                .and_then(Value::as_object)
                .and_then(|obj| obj.get(&category))
                .and_then(Value::as_object)
                .and_then(|obj| obj.get(&field))
                .ok_or(OdysseyError::configuration_error(
                    format!("No such configuration field {category}.{field}").into(),
                    404,
                ))?
                .clone(),
        ))
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
