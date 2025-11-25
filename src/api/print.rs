use std::sync::Arc;

use poem::{web::Data, Result};
use poem_openapi::{param::Query, OpenApi};
use tokio::sync::mpsc;
use tracing::instrument;

use crate::{
    api::Api, api_objects::LocationCategory, configuration::Configuration, printer::Operation,
};

#[derive(Debug)]
pub struct PrintApi;

#[OpenApi(prefix_path = "/print")]
impl PrintApi {
    #[instrument(ret, skip(operation_sender, configuration))]
    #[oai(path = "/start", method = "post")]
    async fn start_print(
        &self,
        Query(file_path): Query<String>,
        Query(location): Query<Option<LocationCategory>>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<()> {
        let location = location.unwrap_or(LocationCategory::Local);

        let file_data = Api::_get_filedata(&file_path, location, &configuration.api)?;

        Ok(
            Api::send_statemachine_operation(operation_sender, Operation::StartPrint { file_data })
                .await?,
        )
    }

    #[instrument(ret, skip(operation_sender))]
    #[oai(path = "/pause", method = "post")]
    async fn pause_print(
        &self,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        Ok(Api::send_statemachine_operation(operation_sender, Operation::PausePrint {}).await?)
    }

    #[instrument(ret, skip(operation_sender))]
    #[oai(path = "/resume", method = "post")]
    async fn resume_print(
        &self,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        Ok(Api::send_statemachine_operation(operation_sender, Operation::ResumePrint {}).await?)
    }

    #[instrument(ret, skip(operation_sender))]
    #[oai(path = "/cancel", method = "post")]
    async fn cancel_print(
        &self,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        Ok(Api::send_statemachine_operation(operation_sender, Operation::StopPrint {}).await?)
    }
}
