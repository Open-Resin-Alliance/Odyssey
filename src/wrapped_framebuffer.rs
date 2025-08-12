use std::{fs::OpenOptions, io::Write};

use framebuffer::Framebuffer;

// Wrap the real framebuffer in this, with a corresponding write call
// If none, write to file at given path instead, so we can pretend it's real
// and examine the output manually if we like
pub struct WrappedFramebuffer {
    pub frame_buffer: Option<Framebuffer>,
    pub fb_path: String,
}

impl WrappedFramebuffer {
    ///Writes a frame to the Framebuffer, or to the fb_path if not a real buffer
    pub fn write_frame(&mut self, frame: &[u8]) {
        match self.frame_buffer.as_mut() {
            Some(fb) => fb.write_frame(frame),
            None => {
                tracing::info!("Writing layer to path: {}", self.fb_path);
                match OpenOptions::new()
                    .append(true)
                    .open(self.fb_path.clone())
                    .as_mut()
                {
                    Ok(output_file) => {
                        let _ = output_file.write_all(frame);
                    }
                    Err(e) => tracing::error!("Error while writing layer: {}", e),
                }
            }
        }
    }
}
