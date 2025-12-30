use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use optional_struct::optional_struct;
use poem_openapi::{Enum, Object};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::{configuration::PrintUploadDirectory, error::OdysseyError};

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct FileData {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Enum)]
pub enum FileType {
    Directory,
    UnknownFile,
    SL1,
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct FileMetadata {
    pub path: String,
    pub name: String,
    pub last_modified: Option<u64>,
    pub file_size: u64,
    pub file_type: FileType,
    pub upload_directory: PrintUploadDirectory,
}

impl FileMetadata {
    pub fn from_path(
        file_path: String,
        upload_directory: &PrintUploadDirectory,
    ) -> Result<Self, io::Error>
    where
        Self: Sized,
    {
        let path = Path::new(&upload_directory.path).join(&file_path);

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
            .and_then(|path_str| path_str.to_str())
            .map(|path_str| path_str.to_string())
            .ok_or(io::Error::new(
                io::ErrorKind::NotFound,
                "Unable to parse file name",
            ))?;

        Ok(FileMetadata {
            path: file_path,
            name,
            last_modified: modified_time,
            file_size,
            file_type,
            upload_directory: upload_directory.clone(),
        })
    }
    pub fn get_full_path(&self) -> PathBuf {
        Path::new(self.upload_directory.path.as_str()).join(self.path.as_str())
    }
    pub fn open_file(&self) -> Result<File, OdysseyError> {
        Ok(File::open(self.get_full_path())?)
    }
    pub async fn delete_file(&self) -> Result<(), OdysseyError> {
        match self.file_type {
            FileType::Directory => fs::remove_dir(self.get_full_path()).await?,
            _ => fs::remove_file(self.get_full_path()).await?,
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct PrintMetadata {
    pub file_data: FileMetadata,
    pub used_material: f64,
    pub print_time: f64,
    pub layer_height: f64,
    pub layer_height_microns: u32,
    pub layer_count: usize,
    pub user_metadata: PrintUserMetadata,
}

#[optional_struct(UpdatePrintUserMetadata)]
#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct PrintUserMetadata {
    pub print_count: u32,
    pub favorite: bool,
    pub rating: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Enum)]
pub enum ThumbnailSize {
    Large,
    Small,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Object)]
pub struct PhysicalState {
    pub z: f64,
    pub z_microns: u32,
    pub curing: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct PrinterState {
    pub print_data: Option<PrintMetadata>,
    pub paused: Option<bool>,
    pub layer: Option<usize>,
    pub physical_state: PhysicalState,
    pub status: PrinterStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize, Enum)]
pub enum PrinterStatus {
    Printing,
    Idle,
    Shutdown,
}

#[derive(Clone, Debug, Serialize, Deserialize, Enum)]
pub enum DisplayTest {
    White,
    Blank,
    Grid,
    Dimensions,
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct ReleaseVersion {
    pub name: String,
    pub version: String,
    pub date: String,
    pub body: Option<String>,
}
