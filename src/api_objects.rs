use std::{
    error::Error,
    ffi::{OsStr, OsString},
    fs::File,
    io,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use optional_struct::optional_struct;
use poem::error::NotFoundError;
use poem_openapi::{Enum, Object};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Enum)]
pub enum LocationCategory {
    Local,
    Usb,
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct FileData {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct FileMetadata {
    pub path: String,
    pub name: String,
    pub last_modified: Option<u64>,
    pub file_size: Option<u64>,
    pub location_category: LocationCategory,
    pub parent_path: String,
}

impl FileMetadata {
    pub fn from_path(
        file_path: &str,
        parent_path: &str,
        location: LocationCategory,
    ) -> Result<Self, io::Error>
    where
        Self: Sized,
    {
        let path = Path::new(parent_path).join(file_path);

        let metadata = path.metadata().ok();

        let modified_time = metadata
            .as_ref()
            .and_then(|meta| meta.modified().ok())
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|dur| dur.as_secs());

        let file_size = metadata.as_ref().map(|meta| meta.len());

        let name = path
            .file_name()
            .and_then(|path_str| path_str.to_str())
            .map(|path_str| path_str.to_string())
            .ok_or(io::Error::new(
                io::ErrorKind::NotFound,
                "Unable to parse file name",
            ))?;

        Ok(FileMetadata {
            path: file_path.to_owned(),
            name,
            last_modified: modified_time,
            file_size,
            location_category: location,
            parent_path: parent_path.to_string(),
        })
    }
    pub fn get_full_path(&self) -> PathBuf {
        Path::new(self.parent_path.as_str()).join(self.path.as_str())
    }
    pub fn open_file(&self) -> Result<File, io::Error> {
        File::open(self.get_full_path())
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
