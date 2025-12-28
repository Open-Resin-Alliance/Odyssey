use std::sync::Arc;

use poem::{web::Data, Result};
use poem_openapi::{param::Query, OpenApi};
use tokio::sync::mpsc;
use tracing::instrument;

use crate::{
    api::Api,
    api_objects::{DisplayTest},
    configuration::{Configuration, PrintUploadDirectory},
    printer::Operation,
};

#[derive(Debug)]
pub struct ManualApi;

#[OpenApi(prefix_path = "/manual")]
impl ManualApi {
    #[instrument(ret, skip(operation_sender))]
    #[oai(path = "/", method = "post")]
    async fn manual_control(
        &self,
        Query(z): Query<Option<f64>>,
        Query(cure): Query<Option<bool>>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        if let Some(z) = z {
            Api::send_statemachine_operation(
                operation_sender,
                Operation::ManualMove {
                    z: (z * 1000.0).trunc() as u32,
                },
            )
            .await?;
        }

        if let Some(cure) = cure {
            Api::send_statemachine_operation(operation_sender, Operation::ManualCure { cure })
                .await?;
        }

        Ok(())
    }
    #[instrument(ret, skip(operation_sender))]
    #[oai(path = "/home", method = "post")]
    async fn manual_home(
        &self,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        Ok(Api::send_statemachine_operation(operation_sender, Operation::ManualHome).await?)
    }
    #[instrument(ret, skip(operation_sender))]
    #[oai(path = "/hardware_command", method = "post")]
    async fn manual_command(
        &self,
        Query(command): Query<String>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        Ok(
            Api::send_statemachine_operation(
                operation_sender,
                Operation::ManualCommand { command },
            )
            .await?,
        )
    }
    #[instrument(ret, skip(operation_sender))]
    #[oai(path = "/display_test", method = "post")]
    async fn manual_display_test(
        &self,
        Query(test): Query<DisplayTest>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        Ok(Api::send_statemachine_operation(
            operation_sender,
            Operation::ManualDisplayTest { test },
        )
        .await?)
    }
    #[instrument(ret, skip(configuration, operation_sender))]
    #[oai(path = "/display_layer", method = "post")]
    async fn manual_display_layer(
        &self,
        Query(file_path): Query<String>,
        Query(print_upload_directory): Query<Option<PrintUploadDirectory>>,
        Query(layer): Query<usize>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<()> {
        let print_upload_directory = print_upload_directory.unwrap_or(LocationCategory::Local);

        let file_data = Api::_get_filedata(&file_path, location, &configuration.api)?;

        Ok(Api::send_statemachine_operation(
            operation_sender,
            Operation::ManualDisplayLayer { file_data, layer },
        )
        .await?)
    }
}
