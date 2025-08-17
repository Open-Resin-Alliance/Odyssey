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

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct PrintMetadata {
    pub file_data: FileMetadata,
    pub used_material: f64,
    pub print_time: f64,
    pub layer_height: f64,
    pub layer_height_microns: u32,
    pub layer_count: usize,
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
