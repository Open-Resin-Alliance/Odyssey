use std::sync::Arc;

use poem::{web::Data, Result};
use poem_openapi::{param::Query, OpenApi};
use tokio::sync::mpsc;
use tracing::instrument;

use crate::{api::Api, configuration::Configuration, printer::Operation};

#[derive(Debug)]
pub struct PrintApi;

#[OpenApi(prefix_path = "/print")]
impl PrintApi {
    #[instrument(ret, skip(operation_sender, configuration))]
    #[oai(path = "/start/:sub", method = "post")]
    async fn start_print(
        &self,
        Query(directory_label): Query<Option<String>>,
        Query(subdirectory): Query<Option<String>>,
        Query(filename): Query<String>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<()> {
        let print_upload_directory = configuration.api.get_print_upload_dir(&directory_label)?;

        let file_data = print_upload_directory.get_file_from_subdir(&filename, subdirectory)?;

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
