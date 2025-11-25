use std::{
    ffi::OsStr,
    fs::File,
    io::{Error, ErrorKind, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use futures::StreamExt;
use glob::glob;
use itertools::Itertools;
use poem::{
    error::{
        BadRequest, GetDataError, InternalServerError, MethodNotAllowedError, NotFound,
        NotImplemented, Unauthorized,
    },
    web::Data,
    EndpointExt, Result,
};
use poem_openapi::{
    param::Query,
    payload::{Attachment, Json},
    types::multipart::Upload,
    Multipart, Object, OpenApi,
};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::instrument;

use crate::{
    api_objects::{
        FileMetadata, LocationCategory, PrintMetadata, ThumbnailSize, UpdatePrintUserMetadata,
    },
    configuration::{ApiConfig, Configuration},
    printfile::PrintFile,
    sl1::Sl1,
};

#[derive(Debug)]
pub struct FilesApi;

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
#[OpenApi(prefix_path = "/files")]
impl FilesApi {
    #[instrument(ret, skip(configuration))]
    #[oai(path = "/", method = "post")]
    async fn upload_file(
        &self,
        file_upload: UploadPayload,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<()> {
        tracing::info!("Uploading file");

        let file_name = file_upload
            .file
            .file_name()
            .map(|s| s.to_string().clone())
            .ok_or(BadRequest(GetDataError("Could not get file name")))?;

        let bytes = file_upload.file.into_vec().await.map_err(BadRequest)?;

        let mut f = File::create(format!("{}/{file_name}", configuration.api.upload_path))
            .map_err(InternalServerError)?;
        f.write_all(bytes.as_slice()).map_err(InternalServerError)?;

        Ok(())
    }
    #[instrument(ret, skip(configuration))]
    #[oai(path = "/", method = "get")]
    async fn get_files(
        &self,
        Query(subdirectory): Query<Option<String>>,
        Query(location): Query<Option<LocationCategory>>,
        Query(page_index): Query<Option<usize>>,
        Query(page_size): Query<Option<usize>>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<Json<FilesResponse>> {
        let location = location.unwrap_or(LocationCategory::Local);
        let page_index = page_index.unwrap_or(DEFAULT_PAGE_INDEX);
        let page_size = page_size.unwrap_or(DEFAULT_PAGE_SIZE);

        match location {
            LocationCategory::Local => {
                Self::_get_local_files(subdirectory, page_index, page_size, &configuration.api)
            }
            LocationCategory::Usb => {
                Self::_get_usb_files(page_index, page_size, &configuration.api)
            }
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
            .flat_map(|f| Self::_get_filedata(f, LocationCategory::Local, configuration).ok())
            .collect_vec();
        let files = paths
            .iter()
            .filter(|f| !f.is_dir())
            .filter_map(|f| f.as_os_str().to_str())
            .flat_map(|f| Self::_get_print_metadata(f, LocationCategory::Local, configuration).ok())
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
            LocationCategory::Usb => Self::get_usb_file_path(&configuration.usb_glob, file_path),
            LocationCategory::Local => {
                Self::get_local_file_path(&configuration.upload_path, file_path)
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
        let file_data = Self::_get_filedata(file_path, location, configuration)?;
        tracing::info!("Extracting print metadata");

        Ok(Sl1::from_file(file_data).map_err(NotFound)?.get_metadata())
    }
    #[instrument(ret, skip(configuration))]
    #[oai(path = "/file", method = "get")]
    async fn get_file(
        &self,
        Query(file_path): Query<String>,
        Query(location): Query<Option<LocationCategory>>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<Attachment<Vec<u8>>> {
        let location = location.unwrap_or(LocationCategory::Local);

        tracing::info!("Getting file {:?} in {:?}", file_path, location);

        let full_file_path = Self::get_file_path(&configuration.api, &file_path, &location)?;

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
    #[instrument(ret, skip(configuration))]
    #[oai(path = "/file/metadata", method = "get")]
    async fn get_file_metadata(
        &self,
        Query(file_path): Query<String>,
        Query(location): Query<Option<LocationCategory>>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<Json<PrintMetadata>> {
        let location = location.unwrap_or(LocationCategory::Local);

        Ok(Json(Self::_get_print_metadata(
            &file_path,
            location,
            &configuration.api,
        )?))
    }

    #[instrument(ret, skip(configuration))]
    #[oai(path = "/file/metadata", method = "patch")]
    async fn patch_file_metadata(
        &self,
        Query(file_path): Query<String>,
        Query(location): Query<Option<LocationCategory>>,
        Json(patch_metadata): Json<UpdatePrintUserMetadata>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<Json<PrintMetadata>> {
        let location = location.unwrap_or(LocationCategory::Local);

        tracing::info!(
            "Getting file metadata from {:?} in {:?}",
            file_path,
            location
        );

        let file_data = Self::_get_filedata(&file_path, location, &configuration.api)?;
        tracing::info!("Extracting print metadata");

        Sl1::set_user_metadata(&file_data.open_file().map_err(NotFound)?, patch_metadata)
            .map_err(InternalServerError)?;

        Ok(Json(
            Sl1::from_file(file_data).map_err(NotFound)?.get_metadata(),
        ))
    }

    #[instrument(ret, skip(configuration))]
    #[oai(path = "/file/thumbnail", method = "get")]
    async fn get_thumbnail(
        &self,
        Query(file_path): Query<String>,
        Query(location): Query<Option<LocationCategory>>,
        Query(size): Query<Option<ThumbnailSize>>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<Attachment<Vec<u8>>> {
        let location = location.unwrap_or(LocationCategory::Local);
        let size = size.unwrap_or(ThumbnailSize::Small);

        tracing::info!("Getting thumbnail from {:?} in {:?}", file_path, location);

        let file_metadata = Self::_get_filedata(&file_path, location, &configuration.api)?;
        tracing::info!("Extracting print thumbnail");

        let file_data = Sl1::from_file(file_metadata)
            .map_err(NotFound)?
            .get_thumbnail(size)
            .map_err(InternalServerError)?;

        Ok(Attachment::new(file_data.data).filename(file_data.name))
    }

    #[instrument(ret, skip(configuration))]
    #[oai(path = "/file", method = "delete")]
    async fn delete_file(
        &self,
        Query(file_path): Query<String>,
        Query(location): Query<Option<LocationCategory>>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<Json<FileMetadata>> {
        let location = location.unwrap_or(LocationCategory::Local);
        tracing::info!("Deleting file {:?} in {:?}", file_path, location);

        let metadata = Self::_get_filedata(&file_path, location, &configuration.api)?;
        let full_file_path = metadata.get_full_path();

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
