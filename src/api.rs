mod config;
mod files;
mod manual;
mod print;
mod update;

use std::{sync::Arc, time::Duration};

use futures::{stream::BoxStream, StreamExt};
use poem::{
    listener::TcpListener,
    middleware::Cors,
    web::{sse::Event, Data},
    EndpointExt, Result, Route, Server,
};
use poem_openapi::{
    payload::{EventStream, Json},
    types::ToJSON,
    OpenApi, OpenApiService,
};
use tokio::sync::{mpsc, watch, RwLock};
use tokio_stream::wrappers::WatchStream;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::{
    api_objects::{
        ExecutableVersion, PhysicalState, PrinterState, PrinterStatus,
    },
    configuration::Configuration,
    error::OdysseyError,
    printer::Operation,
    COMMIT_HASH, COMPILE_TARGET, VERSION,
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
    async fn version(&self) -> Json<ExecutableVersion> {
        Json(ExecutableVersion {
            version: VERSION.to_string(),
            compile_target: COMPILE_TARGET.to_string(),
            commit_hash: COMMIT_HASH.to_string(),
        })
    }

    #[instrument(ret, skip(state_receiver))]
    #[oai(path = "/status", method = "get")]
    async fn get_status(
        &self,
        Data(state_receiver): Data<&watch::Receiver<PrinterState>>,
    ) -> Json<PrinterState> {
        Json(state_receiver.borrow().clone())
    }

    #[instrument(skip(state_receiver))]
    #[oai(path = "/status/stream", method = "get")]
    async fn status_stream(
        &self,
        Data(state_receiver): Data<&watch::Receiver<PrinterState>>,
    ) -> EventStream<BoxStream<'static, PrinterState>> {
        EventStream::new(Api::_status_stream(state_receiver))
            .keep_alive(Duration::from_secs(15))
            .to_event(|status| Event::message(status.to_json_string()).event_type("status"))
    }

    fn _status_stream(
        state_receiver: &watch::Receiver<PrinterState>,
    ) -> BoxStream<'static, PrinterState> {
        WatchStream::new(state_receiver.clone()).boxed()
    }
}

pub async fn start_api(
    full_config: Arc<Configuration>,
    operation_sender: mpsc::Sender<Operation>,
    state_receiver: watch::Receiver<PrinterState>,
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
        .data(state_receiver)
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
