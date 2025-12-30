use std::{
    fs::File,
    io::{Read, Write},
    sync::Arc,
};

use poem::{
    error::{BadRequest, GetDataError, InternalServerError},
    web::Data,
    Result,
};
use poem_openapi::{
    param::{Path as PathParam, Query},
    payload::{Attachment, Json},
    types::multipart::Upload,
    Multipart, OpenApi,
};
use tracing::instrument;

use crate::{
    api_objects::{FileMetadata, PrintMetadata, ThumbnailSize, UpdatePrintUserMetadata},
    configuration::Configuration,
    error::OdysseyError,
    printfile::PrintFile,
    uploads::FilesResponse,
};

#[derive(Debug)]
pub struct FilesApi;

#[derive(Debug, Multipart)]
struct UploadPayload {
    file: Upload,
}

#[OpenApi]
impl FilesApi {
    #[instrument(ret, skip(configuration))]
    #[oai(path = "/files", method = "post")]
    async fn upload_file(
        &self,
        file_upload: UploadPayload,
        PathParam(directory_label): PathParam<Option<String>>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<()> {
        tracing::info!("Uploading file");

        let print_upload_dir = configuration.api.get_print_upload_dir(&directory_label)?;

        let file_name = file_upload
            .file
            .file_name()
            .map(|s| s.to_string().clone())
            .ok_or(BadRequest(GetDataError("Could not get file name")))?;

        let bytes = file_upload.file.into_vec().await.map_err(BadRequest)?;

        let mut f = File::create(format!("{0}/{file_name}", print_upload_dir.path))
            .map_err(InternalServerError)?;
        f.write_all(bytes.as_slice()).map_err(InternalServerError)?;

        Ok(())
    }

    #[instrument(ret, skip(configuration))]
    #[oai(path = "/files/", method = "get")]
    async fn get_files_from_default_dir(
        &self,
        Query(page_index): Query<Option<usize>>,
        Query(page_size): Query<Option<usize>>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<Json<FilesResponse>> {
        Ok(FilesApi::_get_files(None, None, page_index, page_size, configuration).map(Json)?)
    }

    #[instrument(ret, skip(configuration))]
    #[oai(path = "/files/:directory_label/", method = "get")]
    async fn get_files_from_dir(
        &self,
        PathParam(directory_label): PathParam<String>,
        Query(page_index): Query<Option<usize>>,
        Query(page_size): Query<Option<usize>>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<Json<FilesResponse>> {
        Ok(FilesApi::_get_files(
            Some(directory_label),
            None,
            page_index,
            page_size,
            configuration,
        )
        .map(Json)?)
    }

    #[instrument(ret, skip(configuration))]
    #[oai(path = "/files/:directory_label/:subdirectory", method = "get")]
    async fn get_files_from_subdir(
        &self,
        PathParam(directory_label): PathParam<String>,
        PathParam(subdirectory): PathParam<String>,
        Query(page_index): Query<Option<usize>>,
        Query(page_size): Query<Option<usize>>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<Json<FilesResponse>> {
        Ok(FilesApi::_get_files(
            Some(directory_label),
            Some(subdirectory),
            page_index,
            page_size,
            configuration,
        )
        .map(Json)?)
    }

    fn _get_files(
        directory_label: Option<String>,
        subdirectory: Option<String>,
        page_index: Option<usize>,
        page_size: Option<usize>,
        configuration: &Arc<Configuration>,
    ) -> Result<FilesResponse, OdysseyError> {
        let print_upload_dir = configuration.api.get_print_upload_dir(&directory_label)?;

        print_upload_dir.get_files(subdirectory, page_index, page_size)
    }

    #[instrument(ret, skip(configuration))]
    #[oai(
        path = "/file/:directory_label/:subdirectory/:filename",
        method = "get"
    )]
    async fn get_file(
        &self,
        PathParam(directory_label): PathParam<Option<String>>,
        PathParam(subdirectory): PathParam<Option<String>>,
        PathParam(filename): PathParam<String>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<Attachment<Vec<u8>>> {
        let print_upload_directory = configuration.api.get_print_upload_dir(&directory_label)?;

        let file_data = print_upload_directory.get_file_from_subdir(&filename, subdirectory)?;

        let mut open_file = file_data.open_file()?;

        let mut data: Vec<u8> = vec![];
        open_file
            .read_to_end(&mut data)
            .map_err(InternalServerError)?;

        Ok(Attachment::new(data).filename(filename))
    }
    #[instrument(ret, skip(configuration))]
    #[oai(
        path = "/file/:directory_label/:subdirectory/:filename/metadata",
        method = "get"
    )]
    async fn get_file_metadata(
        &self,
        PathParam(directory_label): PathParam<Option<String>>,
        PathParam(subdirectory): PathParam<Option<String>>,
        PathParam(filename): PathParam<String>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<Json<PrintMetadata>> {
        let print_upload_directory = configuration.api.get_print_upload_dir(&directory_label)?;

        let file_data = print_upload_directory.get_file_from_subdir(&filename, subdirectory)?;

        Ok(Json(
            TryInto::<Box<dyn PrintFile + Send + Sync>>::try_into(file_data)?.get_metadata(),
        ))
    }

    #[instrument(ret, skip(configuration))]
    #[oai(
        path = "/file/:directory_label/:subdirectory/:filename/metadata",
        method = "patch"
    )]
    async fn patch_file_metadata(
        &self,
        PathParam(directory_label): PathParam<Option<String>>,
        PathParam(subdirectory): PathParam<Option<String>>,
        PathParam(filename): PathParam<String>,
        Json(patch_metadata): Json<UpdatePrintUserMetadata>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<Json<PrintMetadata>> {
        let print_upload_directory = configuration.api.get_print_upload_dir(&directory_label)?;

        let file_data = print_upload_directory.get_file_from_subdir(&filename, subdirectory)?;

        let print_file: Box<dyn PrintFile + Send + Sync> = file_data.try_into()?;

        print_file.set_user_metadata(patch_metadata)?;

        Ok(Json(
            // Fully refetch metadata after operation
            TryInto::<Box<dyn PrintFile + Send + Sync>>::try_into(
                print_file.get_metadata().file_data,
            )?
            .get_metadata(),
        ))
    }

    #[instrument(ret, skip(configuration))]
    #[oai(
        path = "/file/:directory_label/:subdirectory/:filename/thumbnail",
        method = "get"
    )]
    async fn get_thumbnail(
        &self,
        PathParam(directory_label): PathParam<Option<String>>,
        PathParam(subdirectory): PathParam<Option<String>>,
        PathParam(filename): PathParam<String>,
        Query(size): Query<Option<ThumbnailSize>>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<Attachment<Vec<u8>>> {
        let size = size.unwrap_or(ThumbnailSize::Small);

        let print_upload_directory = configuration.api.get_print_upload_dir(&directory_label)?;

        let file_data = print_upload_directory.get_file_from_subdir(&filename, subdirectory)?;

        let mut print_file: Box<dyn PrintFile + Send + Sync> = file_data.try_into()?;

        let file_data = print_file.get_thumbnail(size)?;

        Ok(Attachment::new(file_data.data).filename(file_data.name))
    }

    #[instrument(ret, skip(configuration))]
    #[oai(
        path = "/file/:directory_label/:subdirectory/:filename/thumbnail",
        method = "delete"
    )]
    async fn delete_file(
        &self,
        PathParam(directory_label): PathParam<Option<String>>,
        PathParam(subdirectory): PathParam<Option<String>>,
        PathParam(filename): PathParam<String>,
        Data(configuration): Data<&Arc<Configuration>>,
    ) -> Result<Json<FileMetadata>> {
        let print_upload_directory = configuration.api.get_print_upload_dir(&directory_label)?;

        let file_data = print_upload_directory.get_file_from_subdir(&filename, subdirectory)?;

        file_data.delete_file().await?;

        Ok(Json(file_data))
    }
}
