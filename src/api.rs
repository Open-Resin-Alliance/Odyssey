mod config;
mod files;
mod manual;
mod print;
mod update;

use std::{sync::Arc, time::Duration};

use futures::{stream::BoxStream, StreamExt};
use poem::{
    error::NotFound,
    listener::TcpListener,
    middleware::Cors,
    web::{sse::Event, Data},
    EndpointExt, Result, Route, Server,
};
use poem_openapi::{
    payload::{EventStream, Json, PlainText},
    types::ToJSON,
    OpenApi, OpenApiService,
};
use tokio::{
    sync::{broadcast, mpsc, RwLock},
    time::interval,
};
use tokio_stream::wrappers::BroadcastStream;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::{
    api_objects::{
        FileMetadata, LocationCategory, PhysicalState, PrintMetadata, PrinterState, PrinterStatus,
    },
    configuration::{ApiConfig, Configuration},
    error::OdysseyError,
    printer::Operation,
    printfile::PrintFile,
    sl1::Sl1,
    VERSION,
};

#[derive(Debug)]
struct Api;

#[OpenApi]
impl Api {
    #[instrument(ret, skip(operation_sender))]
    #[oai(path = "/shutdown", method = "post")]
    async fn shutdown(
        &self,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
        Data(cancellation_token): Data<&CancellationToken>,
    ) -> Result<()> {
        Self::send_statemachine_operation(operation_sender, Operation::Shutdown {}).await?;
        cancellation_token.cancel();
        Ok(())
    }

    async fn send_statemachine_operation(
        operation_sender: &mpsc::Sender<Operation>,
        operation: Operation,
    ) -> Result<(), OdysseyError> {
        operation_sender
            .send(operation)
            .await
            .map_err(OdysseyError::from)
    }

    #[instrument(ret)]
    #[oai(path = "/version", method = "get")]
    async fn version(&self) -> PlainText<String> {
        PlainText(VERSION.to_string())
    }

    #[instrument(ret, skip(state_ref))]
    #[oai(path = "/status", method = "get")]
    async fn get_status(
        &self,
        Data(state_ref): Data<&Arc<RwLock<PrinterState>>>,
    ) -> Json<PrinterState> {
        Json(state_ref.read().await.clone())
    }

    #[instrument(skip(state_receiver))]
    #[oai(path = "/status/stream", method = "get")]
    async fn status_stream(
        &self,
        Data(state_receiver): Data<&Arc<broadcast::Receiver<PrinterState>>>,
    ) -> EventStream<BoxStream<'static, Option<PrinterState>>> {
        EventStream::new(Api::_status_stream(state_receiver))
            .keep_alive(Duration::from_secs(15))
            .to_event(|status| match status {
                Some(status_update) => {
                    Event::message(status_update.to_json_string()).event_type("status")
                }
                None => Event::Retry { retry: 1 },
            })
    }

    fn _status_stream(
        state_receiver: &Arc<broadcast::Receiver<PrinterState>>,
    ) -> BoxStream<'static, Option<PrinterState>> {
        BroadcastStream::new(state_receiver.resubscribe())
            .map(|result| result.ok())
            .boxed()
    }

    fn _get_filedata(
        file_path: &str,
        location: LocationCategory,
        configuration: &ApiConfig,
    ) -> Result<FileMetadata> {
        tracing::info!("Getting file data");

        // TODO handle USB _get_filedata
        FileMetadata::from_path(file_path, &configuration.upload_path, location).map_err(NotFound)
    }

    fn _get_print_metadata(
        file_path: &str,
        location: LocationCategory,
        configuration: &ApiConfig,
    ) -> Result<PrintMetadata> {
        let file_data = Api::_get_filedata(file_path, location, configuration)?;
        tracing::info!("Extracting print metadata");

        Ok(Sl1::from_file(file_data).map_err(NotFound)?.get_metadata())
    }
}

async fn run_state_listener(
    mut state_receiver: broadcast::Receiver<PrinterState>,
    state_ref: Arc<RwLock<PrinterState>>,
) {
    let mut interv = interval(Duration::from_millis(1000));

    let mut state: Result<PrinterState, broadcast::error::TryRecvError>;

    loop {
        state = state_receiver.try_recv();
        if state.is_ok() {
            let mut state_data = state_ref.write().await;
            *state_data = state.clone().unwrap();
        }

        interv.tick().await;
    }
}

pub async fn start_api(
    full_config: Arc<Configuration>,
    operation_sender: mpsc::Sender<Operation>,
    state_receiver: broadcast::Receiver<PrinterState>,
    cancellation_token: CancellationToken,
) {
    let state_ref = Arc::new(RwLock::new(PrinterState {
        print_data: None,
        paused: None,
        layer: None,
        physical_state: PhysicalState {
            z: 0.0,
            z_microns: 0,
            curing: false,
        },
        status: PrinterStatus::Shutdown,
    }));

    tokio::spawn(run_state_listener(
        state_receiver.resubscribe(),
        state_ref.clone(),
    ));

    let addr = format!("0.0.0.0:{0}", full_config.api.port);

    let api_service = OpenApiService::new(
        (
            Api,
            files::FilesApi,
            manual::ManualApi,
            update::UpdateApi,
            print::PrintApi,
            config::ConfigApi,
        ),
        "Odyssey API",
        "1.0",
    );

    let ui = api_service.swagger_ui();

    let mut app = Route::new().nest("/", api_service);

    if full_config.api.enable_docs.is_some_and(|enable| enable) || cfg!(debug_assertions) {
        app = app.nest("/docs", ui);
    }

    let api_shutdown_trigger = cancellation_token.clone();

    let app = app
        .data(operation_sender)
        .data(Arc::new(state_receiver))
        .data(state_ref.clone())
        .data(full_config)
        .data(api_shutdown_trigger)
        .with(Cors::new());

    match Server::new(TcpListener::bind(addr))
        .run_with_graceful_shutdown(
            app,
            cancellation_token.clone().cancelled_owned(),
            Option::None,
        )
        .await
    {
        Ok(_) => log::info!("Shutting down API"),
        Err(err) => log::error!(
            "Fatal error encountered while awaiting API shutdown:\n{}",
            err
        ),
    };
}
