use std::{
    ffi::OsStr,
    fs::File,
    io::{Error, ErrorKind, Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

use glob::glob;
use itertools::Itertools;
use poem::{
    error::{
        BadRequest, GetDataError, InternalServerError, MethodNotAllowedError, NotFound,
        NotFoundError, NotImplemented, ServiceUnavailable, Unauthorized,
    },
    listener::TcpListener,
    middleware::Cors,
    web::Data,
    EndpointExt, Result, Route, Server,
};
use poem_openapi::{
    param::Query,
    payload::{Attachment, Json},
    types::multipart::Upload,
    Multipart, Object, OpenApi, OpenApiService,
};
use self_update::update::Release;
use serde::{Deserialize, Serialize};
use tokio::{
    fs,
    sync::{broadcast, mpsc, RwLock},
    time::interval,
};
use tokio_util::sync::CancellationToken;

use crate::{
    api_objects::{
        DisplayTest, FileMetadata, LocationCategory, PhysicalState, PrintMetadata, PrinterState,
        PrinterStatus, ReleaseVersion, ThumbnailSize,
    },
    configuration::{ApiConfig, Configuration},
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

struct Api;

#[OpenApi]
impl Api {
    #[oai(path = "/print/start", method = "post")]
    async fn start_print(
        &self,
        Query(file_path): Query<String>,
        Query(location): Query<Option<LocationCategory>>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
        Data(configuration): Data<&ApiConfig>,
    ) -> Result<()> {
        let location = location.unwrap_or(LocationCategory::Local);

        let full_file_path = Api::get_file_path(configuration, &file_path, &location)?;

        let file_data = Api::_get_filedata(full_file_path, &location, configuration)?;

        operation_sender
            .send(Operation::StartPrint { file_data })
            .await
            .map_err(ServiceUnavailable)
    }

    #[oai(path = "/print/pause", method = "post")]
    async fn pause_print(
        &self,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        operation_sender
            .send(Operation::PausePrint {})
            .await
            .map_err(ServiceUnavailable)
    }

    #[oai(path = "/print/resume", method = "post")]
    async fn resume_print(
        &self,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        operation_sender
            .send(Operation::ResumePrint {})
            .await
            .map_err(ServiceUnavailable)
    }

    #[oai(path = "/print/cancel", method = "post")]
    async fn cancel_print(
        &self,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        operation_sender
            .send(Operation::StopPrint {})
            .await
            .map_err(ServiceUnavailable)
    }

    #[oai(path = "/shutdown", method = "post")]
    async fn shutdown(&self, Data(operation_sender): Data<&mpsc::Sender<Operation>>) -> Result<()> {
        operation_sender
            .send(Operation::Shutdown {})
            .await
            .map_err(ServiceUnavailable)
    }

    #[oai(path = "/status", method = "get")]
    async fn get_status(
        &self,
        Data(state_ref): Data<&Arc<RwLock<PrinterState>>>,
    ) -> Json<PrinterState> {
        Json(state_ref.read().await.clone())
    }

    #[oai(path = "/config", method = "get")]
    async fn get_config(&self, Data(full_config): Data<&Configuration>) -> Json<Configuration> {
        Json(full_config.clone())
    }

    #[oai(path = "/update/releases", method = "get")]
    async fn get_releases(&self) -> Result<Json<Vec<ReleaseVersion>>> {
        Ok(Json(
            updates::get_releases()?
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

    #[oai(path = "/update", method = "post")]
    async fn update(&self,
        Query(release): Query<String>,) -> Result<()> {
       Ok(updates::update(release)?)
    }

    #[oai(path = "/manual", method = "post")]
    async fn manual_control(
        &self,
        z: Query<Option<f64>>,
        cure: Query<Option<bool>>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
        Data(_state_ref): Data<&Arc<RwLock<PrinterState>>>,
    ) -> Result<()> {
        if let Query(Some(z)) = z {
            operation_sender
                .send(Operation::ManualMove {
                    z: (z * 1000.0).trunc() as u32,
                })
                .await
                .map_err(ServiceUnavailable)?;
        }

        if let Query(Some(cure)) = cure {
            operation_sender
                .send(Operation::ManualCure { cure })
                .await
                .map_err(ServiceUnavailable)?;
        }

        Ok(())
    }

    #[oai(path = "/manual/home", method = "post")]
    async fn manual_home(
        &self,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
        Data(_state_ref): Data<&Arc<RwLock<PrinterState>>>,
    ) -> Result<()> {
        operation_sender
            .send(Operation::ManualHome)
            .await
            .map_err(ServiceUnavailable)?;

        Ok(())
    }

    #[oai(path = "/manual/hardware_command", method = "post")]
    async fn manual_command(
        &self,
        Query(command): Query<String>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
        Data(_state_ref): Data<&Arc<RwLock<PrinterState>>>,
    ) -> Result<()> {
        operation_sender
            .send(Operation::ManualCommand { command })
            .await
            .map_err(ServiceUnavailable)?;

        Ok(())
    }

    #[oai(path = "/manual/display_test", method = "post")]
    async fn manual_display_test(
        &self,
        Query(test): Query<DisplayTest>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
    ) -> Result<()> {
        operation_sender
            .send(Operation::ManualDisplayTest { test })
            .await
            .map_err(ServiceUnavailable)?;
        Ok(())
    }

    #[oai(path = "/manual/display_layer", method = "post")]
    async fn manual_display_layer(
        &self,
        Query(file_path): Query<String>,
        Query(location): Query<Option<LocationCategory>>,
        Query(layer): Query<usize>,
        Data(operation_sender): Data<&mpsc::Sender<Operation>>,
        Data(configuration): Data<&ApiConfig>,
    ) -> Result<()> {
        let location = location.unwrap_or(LocationCategory::Local);

        let full_file_path = Api::get_file_path(configuration, &file_path, &location)?;

        let file_data = Api::_get_filedata(full_file_path, &location, configuration)?;

        operation_sender
            .send(Operation::ManualDisplayLayer { file_data, layer })
            .await
            .map_err(ServiceUnavailable)
    }

    #[oai(path = "/files", method = "post")]
    async fn upload_file(
        &self,
        file_upload: UploadPayload,
        Data(configuration): Data<&ApiConfig>,
    ) -> Result<()> {
        log::info!("Uploading file");

        let file_name = file_upload
            .file
            .file_name()
            .map(|s| s.to_string().clone())
            .ok_or(BadRequest(GetDataError("Could not get file name")))?;

        let bytes = file_upload.file.into_vec().await.map_err(BadRequest)?;

        let mut f = File::create(configuration.upload_path.clone() + "/" + &file_name)
            .expect("Could not create new file");
        f.write_all(bytes.as_slice())
            .expect("Failed to write file contents");

        Ok(())
    }

    #[oai(path = "/files", method = "get")]
    async fn get_files(
        &self,
        Query(subdirectory): Query<Option<String>>,
        Query(location): Query<Option<LocationCategory>>,
        Query(page_index): Query<Option<usize>>,
        Query(page_size): Query<Option<usize>>,
        Data(configuration): Data<&ApiConfig>,
    ) -> Result<Json<FilesResponse>> {
        let location = location.unwrap_or(LocationCategory::Local);
        let page_index = page_index.unwrap_or(DEFAULT_PAGE_INDEX);
        let page_size = page_size.unwrap_or(DEFAULT_PAGE_SIZE);

        log::info!(
            "Getting files in location={:?}, subdirectory={:?}, page_index={:?}, page_size={:?}",
            location,
            subdirectory,
            page_index,
            page_size
        );

        match location {
            LocationCategory::Local => {
                Api::_get_local_files(subdirectory, page_index, page_size, configuration)
            }
            LocationCategory::Usb => Api::_get_usb_files(page_index, page_size, configuration),
        }
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
            .map(|f| f.path())
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
            .flat_map(|f| {
                Api::_get_filedata(f.clone(), &LocationCategory::Local, configuration).ok()
            })
            .collect_vec();
        let files = paths
            .iter()
            .filter(|f| !f.is_dir())
            .flat_map(|f| {
                Api::_get_print_metadata(f.clone(), &LocationCategory::Local, configuration).ok()
            })
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
        log::info!("Getting full file path {:?}, {:?}", location, file_path);

        match location {
            LocationCategory::Usb => Api::get_usb_file_path(configuration, file_path),
            LocationCategory::Local => Api::get_local_file_path(configuration, file_path),
        }
    }

    // Since USB paths are specified as a glob, find all and filter to file_name
    fn get_usb_file_path(configuration: &ApiConfig, file_name: &str) -> Result<PathBuf> {
        let paths = glob(&configuration.usb_glob).map_err(InternalServerError)?;

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
    fn get_local_file_path(configuration: &ApiConfig, file_path: &str) -> Result<PathBuf> {
        let path = Path::new(&configuration.upload_path).join(file_path);

        path.exists()
            .then_some(path)
            .ok_or(InternalServerError(Error::new(
                ErrorKind::NotFound,
                "Unable to find local file",
            )))
    }

    fn _get_filedata(
        target_file: PathBuf,
        location: &LocationCategory,
        configuration: &ApiConfig,
    ) -> Result<FileMetadata> {
        log::info!("Getting file data");
        let modified_time = target_file
            .metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|dur| dur.as_secs());

        let file_size = target_file.metadata().ok().map(|meta| meta.size());

        // TODO handle USB _get_filedata
        Ok(FileMetadata {
            path: target_file
                .strip_prefix(configuration.upload_path.as_str())
                .map_err(InternalServerError)?
                .to_str()
                .map(|path_str| path_str.to_string())
                .ok_or(InternalServerError(Error::new(
                    ErrorKind::NotFound,
                    "unable to parse file path",
                )))?,
            name: target_file
                .file_name()
                .and_then(|path_str| path_str.to_str())
                .map(|path_str| path_str.to_string())
                .ok_or(InternalServerError(Error::new(
                    ErrorKind::NotFound,
                    "Unable to parse file name",
                )))?,
            last_modified: modified_time,
            file_size,
            location_category: location.clone(),
            parent_path: configuration.upload_path.clone(),
        })
    }

    fn _get_print_metadata(
        target_file: PathBuf,
        location: &LocationCategory,
        configuration: &ApiConfig,
    ) -> Result<PrintMetadata> {
        let file_data = Api::_get_filedata(target_file, location, configuration)?;
        log::info!("Extracting print metadata");

        Ok(Sl1::from_file(file_data).get_metadata())
    }

    #[oai(path = "/file", method = "get")]
    async fn get_file(
        &self,
        Query(file_path): Query<String>,
        Query(location): Query<Option<LocationCategory>>,
        Data(configuration): Data<&ApiConfig>,
    ) -> Result<Attachment<Vec<u8>>> {
        let location = location.unwrap_or(LocationCategory::Local);

        log::info!("Getting file {:?} in {:?}", file_path, location);

        let full_file_path = Api::get_file_path(configuration, &file_path, &location)?;

        let file_name = full_file_path
            .file_name()
            .and_then(|filestr| filestr.to_str())
            .ok_or(InternalServerError(Error::new(
                ErrorKind::NotFound,
                "unable to parse file path",
            )))?;

        let mut open_file = File::open(full_file_path.clone()).map_err(InternalServerError)?;

        let mut data: Vec<u8> = vec![];
        open_file
            .read_to_end(&mut data)
            .map_err(InternalServerError)?;

        Ok(Attachment::new(data).filename(file_name))
    }

    #[oai(path = "/file/metadata", method = "get")]
    async fn get_file_metadata(
        &self,
        Query(file_path): Query<String>,
        Query(location): Query<Option<LocationCategory>>,
        Data(configuration): Data<&ApiConfig>,
    ) -> Result<Json<PrintMetadata>> {
        let location = location.unwrap_or(LocationCategory::Local);

        log::info!(
            "Getting file metadata from {:?} in {:?}",
            file_path,
            location
        );
        let full_file_path = Api::get_file_path(configuration, &file_path, &location)?;

        Ok(Json(Api::_get_print_metadata(
            full_file_path,
            &location,
            configuration,
        )?))
    }

    #[oai(path = "/file/thumbnail", method = "get")]
    async fn get_thumbnail(
        &self,
        Query(file_path): Query<String>,
        Query(location): Query<Option<LocationCategory>>,
        Query(size): Query<Option<ThumbnailSize>>,
        Data(configuration): Data<&ApiConfig>,
    ) -> Result<Attachment<Vec<u8>>> {
        let location = location.unwrap_or(LocationCategory::Local);
        let size = size.unwrap_or(ThumbnailSize::Small);

        log::info!("Getting thumbnail from {:?} in {:?}", file_path, location);
        let full_file_path = Api::get_file_path(configuration, &file_path, &location)?;

        let file_metadata = Api::_get_filedata(full_file_path, &location, configuration)?;
        log::info!("Extracting print thumbnail");

        let file_data = Sl1::from_file(file_metadata)
            .get_thumbnail(size)
            .map_err(InternalServerError)?;

        Ok(Attachment::new(file_data.data).filename(file_data.name))
    }

    #[oai(path = "/file", method = "delete")]
    async fn delete_file(
        &self,
        Query(file_path): Query<String>,
        Query(location): Query<Option<LocationCategory>>,
        Data(configuration): Data<&ApiConfig>,
    ) -> Result<Json<FileMetadata>> {
        let location = location.unwrap_or(LocationCategory::Local);
        log::info!("Deleting file {:?} in {:?}", file_path, location);

        let full_file_path = Api::get_file_path(configuration, &file_path, &location)?;

        let metadata = Api::_get_filedata(full_file_path.clone(), &location, configuration)?;

        if full_file_path.is_dir() {
            fs::remove_dir_all(full_file_path)
                .await
                .map_err(InternalServerError)?;
        } else {
            fs::remove_file(full_file_path)
                .await
                .map_err(InternalServerError)?;
        }

        Ok(Json(metadata))
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
    full_config: Configuration,
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

    let configuration = full_config.api.clone();

    tokio::spawn(run_state_listener(state_receiver, state_ref.clone()));

    let port = configuration.port.to_string();
    let addr = format!("0.0.0.0:{port}");

    let api_service = OpenApiService::new(Api, "Odyssey API", "1.0");

    let ui = api_service.swagger_ui();

    let mut app = Route::new().nest("/", api_service);

    if cfg!(debug_assertions) {
        app = app.nest("/docs", ui);
    }

    let app = app
        .data(operation_sender)
        .data(state_ref.clone())
        .data(full_config.clone())
        .data(configuration.clone())
        .with(Cors::new());

    Server::new(TcpListener::bind(addr))
        .run_with_graceful_shutdown(
            app,
            cancellation_token.clone().cancelled_owned(),
            Option::None,
        )
        .await
        .expect("Error encountered");
}
