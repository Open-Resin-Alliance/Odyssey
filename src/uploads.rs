use std::{
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use itertools::Itertools;
use poem_openapi::Object;
use serde::{Deserialize, Serialize};

use crate::{
    api_objects::{FileMetadata, FileType, PrintMetadata},
    configuration::PrintUploadDirectory,
    error::OdysseyError,
    printfile::PrintFile,
};

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct FilesResponse {
    pub print_files: Vec<PrintMetadata>,
    pub files: Vec<FileMetadata>,
    pub dirs: Vec<FileMetadata>,
    pub next_index: Option<usize>,
}
const DEFAULT_PAGE_INDEX: usize = 0;
const DEFAULT_PAGE_SIZE: usize = 100;

impl PrintUploadDirectory {
    pub fn get_file_from_subdir(
        &self,
        filename: &str,
        subdirectory: Option<String>,
    ) -> Result<FileMetadata, OdysseyError> {
        let file_path = Path::new(&subdirectory.unwrap_or("".to_string())).join(filename);

        self.get_file_from_pathbuf(&file_path)
    }

    pub fn get_file_from_pathbuf(&self, file_path: &PathBuf) -> Result<FileMetadata, OdysseyError> {
        let path: PathBuf = Path::new(&self.path).join(file_path);

        let metadata = path.metadata()?;

        let file_type = match path.is_dir() {
            true => FileType::Directory,
            false => FileType::from_extension(path.extension().and_then(|os_str| os_str.to_str())),
        };

        let modified_time = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|dur| dur.as_secs());

        let file_size = metadata.len();

        let name = path
            .file_name()
            .ok_or(OdysseyError::file_error(
                format!("{} is not a valid file path", path.to_string_lossy()).into(),
                404,
            ))
            .and_then(|path_str| {
                path_str.to_os_string().into_string().map_err(|_| {
                    OdysseyError::file_error(
                        format!("{} contains non-unicode characters", path.to_string_lossy())
                            .into(),
                        400,
                    )
                })
            })?;

        Ok(FileMetadata {
            path: file_path.to_string_lossy().to_string(),
            name,
            last_modified: modified_time,
            file_size,
            file_type,
            upload_directory: self.clone(),
        })
    }

    fn get_path_iterator(
        &self,
        subdirectory: Option<String>,
    ) -> Result<Box<dyn Iterator<Item = PathBuf>>, OdysseyError> {
        let upload_path = Path::new(&self.path).join(subdirectory.unwrap_or("".to_string()));

        Ok(Box::new(upload_path.read_dir()?.flatten().filter_map(
            move |f| {
                f.path()
                    .strip_prefix(&upload_path)
                    .map(|path_ref| path_ref.to_owned())
                    .ok()
            },
        )))
    }

    pub fn get_files(
        &self,
        subdirectory: Option<String>,
        page_index: Option<usize>,
        page_size: Option<usize>,
    ) -> Result<FilesResponse, OdysseyError> {
        let page_index = page_index.unwrap_or(DEFAULT_PAGE_INDEX);
        let page_size = page_size.unwrap_or(DEFAULT_PAGE_SIZE);

        let mut files = Vec::with_capacity(page_size);
        let mut print_files = Vec::with_capacity(page_size);
        let mut dirs = Vec::with_capacity(page_size);

        // Temporary value to ensure the paged results are not dropped from memory
        let paged_paths = self
            .get_path_iterator(subdirectory)?
            // TODO add sorting here
            .chunks(page_size);

        let mut paged_paths_iter = paged_paths.into_iter();

        if let Some(path_page) = paged_paths_iter.nth(page_index) {
            path_page
                .map(|path| self.get_file_from_pathbuf(&path))
                .for_each(|file_data| {
                    if let Ok(file_data) = file_data {
                        match file_data.file_type {
                            crate::api_objects::FileType::Directory => dirs.push(file_data),
                            crate::api_objects::FileType::SL1 => {
                                if let Ok(print_file) =
                                    TryInto::<Box<dyn PrintFile + Send + Sync>>::try_into(file_data)
                                {
                                    print_files.push(print_file.get_metadata());
                                }
                            }
                            crate::api_objects::FileType::UnknownFile => files.push(file_data),
                        }
                    }
                })
        };

        let next_index = Some(page_index + 1).filter(|_| paged_paths_iter.next().is_some());

        Ok(FilesResponse {
            print_files,
            files,
            dirs,
            next_index,
        })
    }
}
