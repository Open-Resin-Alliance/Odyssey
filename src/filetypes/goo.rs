use std::{
    io::Error,
    fs
};

use goo::{GooFile, LayerDecoder, Run};

use async_trait::async_trait;

use crate::{
    api_objects::{FileData, FileMetadata, PrintMetadata},
    filetypes::printfile::{Layer, PrintFile},
};

/// The sliced .goo-format model
pub struct Goo {}

#[async_trait]
impl PrintFile for Goo {
    fn from_file(file_data: FileMetadata) -> Self {

        
        log::info!("Loading PrintFile from SL1 {:?}", file_data);

        let full_path = Path::new(file_data.parent_path.as_str()).join(file_data.path.as_str());

        let file = File::open(full_path).unwrap();

        let goo = GooFile::deserialize(fs::read(full_path)).unwrap();
        

    }
    async fn get_layer_data(&mut self, index: usize) -> Option<Layer> {}
    fn get_layer_count(&self) -> usize {}
    fn get_layer_height(&self) -> f32 {}
    fn get_metadata(&self) -> PrintMetadata {}
    fn get_thumbnail(&mut self) -> Result<FileData, Error> {}

    fn get_lift(&self) -> Option<f32> {
        None
    }
    fn get_up_speed(&self) -> Option<f32> {
        None
    }
    fn get_down_speed(&self) -> Option<f32> {
        None
    }
    fn get_wait_after_exposure(&self) -> Option<f32> {
        None
    }
    fn get_wait_before_exposure(&self) -> Option<f32> {
        None
    }
}
