use std::{fs::File, path::PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use xattr::FileExt;

use crate::{
    api_objects::{
        FileData, FileMetadata, FileType, PrintMetadata, PrintUserMetadata, ThumbnailSize,
        UpdatePrintUserMetadata,
    },
    error::OdysseyError,
    sl1::Sl1,
};

static XATTR_PRINT_COUNT: &str = "user.odyssey.print_count";
static XATTR_PRINT_RATING: &str = "user.odyssey.print_rating";
static XATTR_PRINT_FAVORITE: &str = "user.odyssey.favorite";

pub static PRINT_FILE_EXTENSIONS: [&str; 1] = [".sl1"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layer {
    pub file_name: String,
    pub data: Vec<u8>,
    pub exposure_time: f64,
}

impl FileType {
    pub fn from_path(path: PathBuf) -> FileType {
        if path.is_dir() {
            FileType::Directory
        } else {
            FileType::from_extension(path.extension().and_then(|ext| ext.to_str()))
        }
    }
    pub fn from_extension(extension: Option<&str>) -> FileType {
        match extension.unwrap_or("").to_lowercase().as_str() {
            ".sl1" => FileType::SL1,
            _ => FileType::UnknownFile,
        }
    }

    pub fn get_printfile(
        &self,
        file_data: FileMetadata,
    ) -> Result<Box<impl PrintFile>, OdysseyError> {
        match file_data.file_type {
            FileType::SL1 => Ok(Box::new(Sl1::try_from(file_data)?)),
            _ => Err(OdysseyError::file_error(
                "Unsupported print file type".into(),
                400,
            )),
        }
    }
}

impl<'a> TryInto<&'a dyn PrintFile> for FileMetadata {
    type Error = OdysseyError;

    fn try_into(self) -> Result<&'a dyn PrintFile, Self::Error> {
        todo!()
    }
}

impl<'a> TryInto<&'a mut dyn PrintFile> for FileMetadata {
    type Error = OdysseyError;

    fn try_into(self) -> Result<&'a mut dyn PrintFile, Self::Error> {
        todo!()
    }
}

impl TryInto<Box<dyn PrintFile + Send + Sync>> for FileMetadata {
    type Error = OdysseyError;

    fn try_into(self) -> Result<Box<dyn PrintFile + Send + Sync>, Self::Error> {
        match self.file_type {
            FileType::SL1 => Ok(Box::new(Sl1::try_from(self)?)),
            _ => Err(OdysseyError::file_error(
                "Unsupported print file type".into(),
                400,
            )),
        }
    }
}

#[async_trait]
pub trait PrintFile {
    async fn get_layer_data(&mut self, index: usize) -> Option<Layer>;
    fn get_layer_count(&self) -> usize;
    fn get_layer_height(&self) -> u32;
    fn get_metadata(&self) -> PrintMetadata;
    fn get_thumbnail(&mut self, size: ThumbnailSize) -> Result<FileData, OdysseyError>;
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
    fn _set_xattr(&self, file: &File, xattr_name: &str, value: &[u8]) -> Result<(), OdysseyError> {
        Ok(file.set_xattr(xattr_name, value)?)
    }
    fn set_user_metadata(
        &self,
        user_metadata: UpdatePrintUserMetadata,
    ) -> Result<(), OdysseyError> {
        let file = &self.get_metadata().file_data.open_file()?;
        let mut result = Ok(());
        if let Some(print_count) = user_metadata.print_count {
            result = result.and(self.set_print_count(file, print_count));
        }
        if let Some(favorite) = user_metadata.favorite {
            result = result.and(self.set_favorite(file, favorite));
        }
        if let Some(rating) = user_metadata.rating {
            result = result.and(self.set_rating(file, rating));
        }
        result
    }
    fn set_print_count(&self, file: &File, val: u32) -> Result<(), OdysseyError> {
        self._set_xattr(file, XATTR_PRINT_COUNT, &val.to_be_bytes())
    }
    fn set_rating(&self, file: &File, val: u8) -> Result<(), OdysseyError> {
        self._set_xattr(file, XATTR_PRINT_RATING, &val.to_be_bytes())
    }
    fn set_favorite(&self, file: &File, val: bool) -> Result<(), OdysseyError> {
        let val: u8 = if val { 1 } else { 0 };
        self._set_xattr(file, XATTR_PRINT_FAVORITE, &val.to_be_bytes())
    }
}
