use std::{fs::File, io::Error};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use xattr::FileExt;

use crate::api_objects::{FileData, FileMetadata, PrintMetadata, ThumbnailSize};

static XATTR_PRINT_COUNT:& str = "user.odyssey.print_count";
static XATTR_PRINT_RATING:& str = "user.odyssey.print_rating";
static XATTR_PRINT_FAVORITE:& str = "user.odyssey.favorite";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layer {
    pub file_name: String,
    pub data: Vec<u8>,
    pub exposure_time: f64,
}

#[async_trait]
pub trait PrintFile {
    fn from_file(file_data: FileMetadata) -> Self
    where
        Self: Sized;
    async fn get_layer_data(&mut self, index: usize) -> Option<Layer>;
    fn get_layer_count(&self) -> usize;
    fn get_layer_height(&self) -> u32;
    fn get_metadata(&self) -> PrintMetadata;
    fn get_thumbnail(&mut self, size: ThumbnailSize) -> Result<FileData, Error>;
    // Optional fields not present in every file type
    fn get_lift(&self) -> Option<u32> {
        None
    }
    fn get_up_speed(&self) -> Option<f64> {
        None
    }
    fn get_down_speed(&self) -> Option<f64> {
        None
    }
    fn get_wait_after_exposure(&self) -> Option<f64> {
        None
    }
    fn get_wait_before_exposure(&self) -> Option<f64> {
        None
    }
    fn _get_xattr(file: &File, xattr_name: &str) -> Option<Vec<u8>> where Self: Sized{
        if xattr::SUPPORTED_PLATFORM {
            match file.get_xattr(xattr_name) {
                Ok(bytes) => return bytes,
                Err(err) => {
                    tracing::warn!("Unable to load xattr {xattr_name}:\n{err}");
                },
            }
        }
        None
    }
    fn get_print_count(file: &File) -> u32 where Self: Sized{
        Self::_get_xattr(file, XATTR_PRINT_COUNT).map(|v| v.try_into().ok()).flatten().map(u32::from_be_bytes).unwrap_or(0)
    }
    fn get_rating(file: &File) -> Option<u8> where Self: Sized{
        Self::_get_xattr(file, XATTR_PRINT_RATING).map(|v| v.try_into().ok()).flatten().map(u8::from_be_bytes)
    }
    fn get_favorite(file: &File) -> bool where Self: Sized{
        Self::_get_xattr(file, XATTR_PRINT_FAVORITE).map(|v| v.try_into().ok()).flatten().map(u8::from_be_bytes).filter(|val|*val!=0).is_some()
    }
}
