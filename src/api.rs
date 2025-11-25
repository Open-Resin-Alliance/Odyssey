mod files;

use std::{
    ffi::OsStr,
    fs::File,
    io::{Error, ErrorKind, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use futures::{stream::BoxStream, StreamExt};
use glob::glob;
use itertools::Itertools;
use optional_struct::Applicable;
use poem::{
    error::{
        BadRequest, GetDataError, InternalServerError, MethodNotAllowedError, NotFound,
        NotImplemented, Unauthorized,
    },
    listener::TcpListener,
    middleware::Cors,
    web::{sse::Event, Data},
    EndpointExt, Result, Route, Server,
};
use poem_openapi::{
    param::Query,
    payload::{Attachment, EventStream, Json},
    types::{multipart::Upload, ToJSON},
    Multipart, Object, OpenApi, OpenApiService,
};
use serde::{Deserialize, Serialize};
use tokio::{
    fs,
    sync::{broadcast, mpsc, RwLock},
    task::spawn_blocking,
    time::interval,
};
use tokio_stream::wrappers::BroadcastStream;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::{
    api_objects::{
        DisplayTest, FileMetadata, LocationCategory, PhysicalState, PrintMetadata, PrinterState,
        PrinterStatus, ReleaseVersion, ThumbnailSize, UpdatePrintUserMetadata,
    },
    configuration::{ApiConfig, Configuration, UpdateConfiguration},
    error::OdysseyError,
    printer::Operation,
    printfile::PrintFile,
    sl1::Sl1,
    updates,
};

#[derive(Debug, Multipart)]
struct UploadPayload {
    file: Upload,
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct FilesResponse {
    pub files: Vec<PrintMetadata>,
    pub dirs: Vec<FileMetadata>,
    pub next_index: Option<usize>,
}

const DEFAULT_PAGE_INDEX: usize = 0;
const DEFAULT_PAGE_SIZE: usize = 100;

#[derive(Debug)]
struct Api;

#[OpenApi]
impl Api {
    #[instrument(ret)]
    #[oai(path = "/print/start", method = "post")]
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
            Self::send_statemachine_operation(
                operation_sender,
                Operation::StartPrint { file_data },
            )
            .await?,
        )
    }

    #[instrument(ret)]
    #[oai(path = "/print/pause", method = "post")]
    async fn pause_print(
        &self,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        Ok(Self::send_statemachine_operation(operation_sender, Operation::PausePrint {}).await?)
    }

    #[instrument(ret)]
    #[oai(path = "/print/resume", method = "post")]
    async fn resume_print(
        &self,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        Ok(Self::send_statemachine_operation(operation_sender, Operation::ResumePrint {}).await?)
    }

    #[instrument(ret)]
    #[oai(path = "/print/cancel", method = "post")]
    async fn cancel_print(
        &self,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        Ok(Self::send_statemachine_operation(operation_sender, Operation::StopPrint {}).await?)
    }

    #[instrument(ret)]
    #[oai(path = "/shutdown", method = "post")]
    async fn shutdown(&self, Data(operation_sender): Data<&mpsc::Sender<Operation>>) -> Result<()> {
        Ok(Self::send_statemachine_operation(operation_sender, Operation::Shutdown {}).await?)
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
    #[oai(path = "/status", method = "get")]
    async fn get_status(
        &self,
        Data(state_ref): Data<&Arc<RwLock<PrinterState>>>,
    ) -> Json<PrinterState> {
        Json(state_ref.read().await.clone())
    }

    #[instrument]
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

    #[instrument(ret)]
    #[oai(path = "/config", method = "get")]
    async fn get_config(
        &self,
        Data(full_config): Data<&Arc<Configuration>>,
    ) -> Json<Configuration> {
        Json(full_config.as_ref().clone())
    }

    #[instrument(ret)]
    #[oai(path = "/config", method = "patch")]
    async fn patch_config(
        &self,
        Data(full_config): Data<&Arc<Configuration>>,
        Json(patch_config): Json<UpdateConfiguration>,
    ) -> Result<Json<Configuration>> {
        let ammend_config = patch_config.build(full_config.as_ref().clone());
        Configuration::overwrite_file(&ammend_config)?;

        Ok(Json(ammend_config))
    }

    #[instrument(ret)]
    #[oai(path = "/update/releases", method = "get")]
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
    #[oai(path = "/update", method = "post")]
    async fn update(&self, Query(release): Query<String>) -> Result<()> {
        Ok(spawn_blocking(|| updates::update(release))
            .await
            .map_err(OdysseyError::from)??)
    }

    #[instrument(ret)]
    #[oai(path = "/manual", method = "post")]
    async fn manual_control(
        &self,
        Query(z): Query<Option<f64>>,
        Query(cure): Query<Option<bool>>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
        Data(_state_ref): Data<&Arc<RwLock<PrinterState>>>,
    ) -> Result<()> {
        if let Some(z) = z {
            Self::send_statemachine_operation(
                operation_sender,
                Operation::ManualMove {
                    z: (z * 1000.0).trunc() as u32,
                },
            )
            .await?;
        }

        if let Some(cure) = cure {
            Self::send_statemachine_operation(operation_sender, Operation::ManualCure { cure })
                .await?;
        }

        Ok(())
    }
    #[instrument(ret)]
    #[oai(path = "/manual/home", method = "post")]
    async fn manual_home(
        &self,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
        Data(_state_ref): Data<&Arc<RwLock<PrinterState>>>,
    ) -> Result<()> {
        Ok(Self::send_statemachine_operation(operation_sender, Operation::ManualHome).await?)
    }
    #[instrument(ret)]
    #[oai(path = "/manual/hardware_command", method = "post")]
    async fn manual_command(
        &self,
        Query(command): Query<String>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
        Data(_state_ref): Data<&Arc<RwLock<PrinterState>>>,
    ) -> Result<()> {
        Ok(Self::send_statemachine_operation(
            operation_sender,
            Operation::ManualCommand { command },
        )
        .await?)
    }
    #[instrument(ret)]
    #[oai(path = "/manual/display_test", method = "post")]
    async fn manual_display_test(
        &self,
        Query(test): Query<DisplayTest>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        Ok(Self::send_statemachine_operation(
            operation_sender,
            Operation::ManualDisplayTest { test },
        )
        .await?)
    }
    #[instrument(ret, skip(configuration))]
    #[oai(path = "/manual/display_layer", method = "post")]
    async fn manual_display_layer(
        &self,
        Query(file_path): Query<String>,
        Query(location): Query<Option<LocationCategory>>,
        Query(layer): Query<usize>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<()> {
        let location = location.unwrap_or(LocationCategory::Local);

        let file_data = Api::_get_filedata(&file_path, location, &configuration.api)?;

        Ok(Self::send_statemachine_operation(
            operation_sender,
            Operation::ManualDisplayLayer { file_data, layer },
        )
        .await?)
    }

    fn _get_local_files(
        subdirectory: Option<String>,
        page_index: usize,
        page_size: usize,
        configuration: &ApiConfig,
    ) -> Result<Json<FilesResponse>> {
        let directory = subdirectory.unwrap_or("".to_string());

        if directory.starts_with('/') || directory.starts_with('.') {
            return Err(Unauthorized(MethodNotAllowedError));
        }

        let upload_string = &configuration.upload_path;

        let upload_path = Path::new(upload_string.as_str());
        let full_path = upload_path.join(directory.as_str());

        let read_dir = full_path.read_dir();

        let files_vec = read_dir
            .map_err(InternalServerError)?
            .flatten()
            .filter_map(|f| {
                f.path()
                    .strip_prefix(upload_path)
                    .map(|path_ref| path_ref.to_owned())
                    .ok()
            })
            // TODO add sorting here
            .filter(|f| f.is_dir() || f.extension().and_then(OsStr::to_str).eq(&Some("sl1")));

        let chunks = files_vec.chunks(page_size);

        let mut chunks_iterator = chunks.into_iter();

        let paths = chunks_iterator
            .nth(page_index)
            .map_or(Vec::new(), |dirs| dirs.collect_vec());

        let dirs = paths
            .iter()
            .filter(|f| f.is_dir())
            .filter_map(|f| f.as_os_str().to_str())
            .flat_map(|f| Api::_get_filedata(f, LocationCategory::Local, configuration).ok())
            .collect_vec();
        let files = paths
            .iter()
            .filter(|f| !f.is_dir())
            .filter_map(|f| f.as_os_str().to_str())
            .flat_map(|f| Api::_get_print_metadata(f, LocationCategory::Local, configuration).ok())
            .collect_vec();

        let next_index = Some(page_index + 1).filter(|_| chunks_iterator.next().is_some());

        Ok(Json(FilesResponse {
            files,
            dirs,
            next_index,
        }))
    }

    fn _get_usb_files(
        _page_index: usize,
        _page_size: usize,
        _configuration: &ApiConfig,
    ) -> Result<Json<FilesResponse>> {
        Err(NotImplemented(MethodNotAllowedError))

        /*
        poem::web::Json(glob(&configuration.usb_glob)
            .expect("Failed to read glob pattern")
            .map(|result| result.expect("Error reading path"))
            .map(|path| path.into_os_string().into_string().expect("Error parsing path"))
            .collect_vec())
        */
    }

    fn get_file_path(
        configuration: &ApiConfig,
        file_path: &str,
        location: &LocationCategory,
    ) -> Result<PathBuf> {
        tracing::info!("Getting full file path {:?}, {:?}", location, file_path);

        match location {
            LocationCategory::Usb => Api::get_usb_file_path(&configuration.usb_glob, file_path),
            LocationCategory::Local => {
                Api::get_local_file_path(&configuration.upload_path, file_path)
            }
        }
    }

    // Since USB paths are specified as a glob, find all and filter to file_name
    fn get_usb_file_path(usb_glob: &str, file_name: &str) -> Result<PathBuf> {
        let paths = glob(usb_glob).map_err(InternalServerError)?;

        let path_buf = paths
            .filter_map(|path| path.ok())
            .find(|path| path.ends_with(file_name))
            .ok_or(InternalServerError(Error::new(
                ErrorKind::NotFound,
                "Unable to find USB file",
            )))?;

        Ok(path_buf)
    }

    // For Local files, look directly for specific file
    fn get_local_file_path(upload_path: &str, file_path: &str) -> Result<PathBuf> {
        let path = Path::new(upload_path).join(file_path);

        path.exists()
            .then_some(path)
            .ok_or(InternalServerError(Error::new(
                ErrorKind::NotFound,
                "Unable to find local file",
            )))
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

    let api_service = OpenApiService::new((Api, files::FilesApi), "Odyssey API", "1.0");

    let ui = api_service.swagger_ui();

    let mut app = Route::new().nest("/", api_service);

    if full_config.api.enable_docs.is_some_and(|enable| enable) || cfg!(debug_assertions) {
        app = app.nest("/docs", ui);
    }

    let app = app
        .data(operation_sender)
        .data(Arc::new(state_receiver))
        .data(state_ref.clone())
        .data(full_config)
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
