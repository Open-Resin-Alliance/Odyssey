use std::{fs::File, io::Error};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use xattr::FileExt;

use crate::api_objects::{
    FileData, FileMetadata, PrintMetadata, PrintUserMetadata, ThumbnailSize,
    UpdatePrintUserMetadata,
};

static XATTR_PRINT_COUNT: &str = "user.odyssey.print_count";
static XATTR_PRINT_RATING: &str = "user.odyssey.print_rating";
static XATTR_PRINT_FAVORITE: &str = "user.odyssey.favorite";

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
    fn _get_xattr(file: &File, xattr_name: &str) -> Option<Vec<u8>>
    where
        Self: Sized,
    {
        if xattr::SUPPORTED_PLATFORM {
            match file.get_xattr(xattr_name) {
                Ok(bytes) => return bytes,
                Err(err) => {
                    tracing::warn!("Unable to load xattr {xattr_name}:\n{err}");
                }
            }
        }
        None
    }
    fn get_user_metadata(file: &File) -> PrintUserMetadata
    where
        Self: Sized,
    {
        PrintUserMetadata {
            print_count: Self::get_print_count(file),
            favorite: Self::get_favorite(file),
            rating: Self::get_rating(file),
        }
    }
    fn get_print_count(file: &File) -> u32
    where
        Self: Sized,
    {
        Self::_get_xattr(file, XATTR_PRINT_COUNT)
            .and_then(|v| v.try_into().ok())
            .map(u32::from_be_bytes)
            .unwrap_or(0)
    }
    fn get_rating(file: &File) -> Option<u8>
    where
        Self: Sized,
    {
        Self::_get_xattr(file, XATTR_PRINT_RATING)
            .and_then(|v| v.try_into().ok())
            .map(u8::from_be_bytes)
    }
    fn get_favorite(file: &File) -> bool
    where
        Self: Sized,
    {
        Self::_get_xattr(file, XATTR_PRINT_FAVORITE)
            .and_then(|v| v.try_into().ok())
            .map(u8::from_be_bytes)
            .filter(|val| *val != 0)
            .is_some()
    }
    fn _set_xattr(file: &File, xattr_name: &str, value: &[u8]) -> Result<(), Error>
    where
        Self: Sized,
    {
        file.set_xattr(xattr_name, value)
    }
    fn set_user_metadata(file: &File, user_metadata: UpdatePrintUserMetadata) -> Result<(), Error>
    where
        Self: Sized,
    {
        let mut result = Ok(());
        if let Some(print_count) = user_metadata.print_count {
            result = result.and(Self::set_print_count(file, print_count));
        }
        if let Some(favorite) = user_metadata.favorite {
            result = result.and(Self::set_favorite(file, favorite));
        }
        if let Some(rating) = user_metadata.rating {
            result = result.and(Self::set_rating(file, rating));
        }
        result
    }
    fn set_print_count(file: &File, val: u32) -> Result<(), Error>
    where
        Self: Sized,
    {
        Self::_set_xattr(file, XATTR_PRINT_COUNT, &val.to_be_bytes())
    }
    fn set_rating(file: &File, val: u8) -> Result<(), Error>
    where
        Self: Sized,
    {
        Self::_set_xattr(file, XATTR_PRINT_RATING, &val.to_be_bytes())
    }
    fn set_favorite(file: &File, val: bool) -> Result<(), Error>
    where
        Self: Sized,
    {
        let val: u8 = if val { 1 } else { 0 };
        Self::_set_xattr(file, XATTR_PRINT_FAVORITE, &val.to_be_bytes())
    }
}
