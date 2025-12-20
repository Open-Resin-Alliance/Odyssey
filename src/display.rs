use framebuffer::Framebuffer;
use png::Decoder;

use crate::{
    api_objects::DisplayTest, configuration::DisplayConfig, wrapped_framebuffer::WrappedFramebuffer,
};

#[derive(Clone)]
pub struct Frame {
    pub file_name: String,
    pub buffer: Vec<u8>,
    pub exposure_time: f64,
    pub bit_depth: u8,
}

impl Frame {
    pub fn from_vec(name: String, exposure_time: f64, data: Vec<u8>) -> Frame {
        let decoder = Decoder::new(data.as_slice());

        let mut png_reader = decoder.read_info().expect("Unable to read PNG metadata");

        let mut f = Frame {
            file_name: name,
            buffer: vec![0; png_reader.output_buffer_size()],
            exposure_time,
            bit_depth: png_reader.info().bit_depth as u8,
        };

        png_reader
            .next_frame(f.buffer.as_mut())
            .expect("Error reading PNG");

        f
    }
}

pub struct PrintDisplay {
    pub frame_buffer: WrappedFramebuffer,
    pub config: DisplayConfig,
}

impl PrintDisplay {
    fn re_encode(&self, buffer: Vec<u8>, bit_depth: u8) -> Vec<u8> {

        return Self::_re_encode(buffer, bit_depth, &self.config.bit_depth)
    }

    fn _re_encode(image_buffer: Vec<u8>, image_bit_depth: u8, display_bit_depths: &Vec<u8>) -> Vec<u8> {
        if display_bit_depths.len() == 1 && display_bit_depths[0] == image_bit_depth {
            return image_buffer;
        }

        let chunk_size: u8 = display_bit_depths.iter().sum(); //8
        let pixels_per_chunk = display_bit_depths.len(); //1
        tracing::info!("Re-encoding frame with bit-depth {} into {} pixels in {} bits, with the following bit layout: {:?}", image_bit_depth, pixels_per_chunk, chunk_size, display_bit_depths);

        let mut new_buffer: Vec<u8> = Vec::new();

        image_buffer
            .chunks_exact(pixels_per_chunk)
            .for_each(|pixel_chunk| {
                // raw binary chunk of pixels, to be broken into bytes and repacked in the Vector later
                let mut raw_chunk = 0b0;
                let mut pos_shift = chunk_size;
                for i in 0..pixels_per_chunk {
                    let depth_difference = image_bit_depth - display_bit_depths[i];
                    pos_shift -= display_bit_depths[i];

                    // Truncate the pixel data to the display's bit depth, then shift it into place in the raw chunk
                    let shifted_pixel: u64 =
                        ((pixel_chunk[i] as u64) >> depth_difference) << (pos_shift);
                    raw_chunk |= shifted_pixel;
                }

                for i in 0..(chunk_size / 8) {
                    // pull the raw chunk back apart into bytes, for push into the new buffer
                    let byte = ((raw_chunk >> (8 * i)) & 0xFF) as u8;
                    new_buffer.push(byte);
                }
            });

        new_buffer
    }

    pub fn display_frame(&mut self, frame: Frame) {
        self.display_bytes(frame.buffer, frame.bit_depth);
    }

    fn display_bytes(&mut self, buffer: Vec<u8>, bit_depth: u8) {
        self.frame_buffer
            .write_frame(&self.re_encode(buffer, bit_depth));
    }

    pub fn display_test(&mut self, test: DisplayTest) {
        let test_bytes = match test {
            DisplayTest::White => self.display_test_white(),
            DisplayTest::Blank => self.display_test_blank(),
            _ => self.display_test_blank(),
        };

        self.display_bytes(test_bytes, 8);
    }

    fn display_test_white(&mut self) -> Vec<u8> {
        vec![0xFF; (self.config.screen_width * self.config.screen_height) as usize]
    }

    fn display_test_blank(&mut self) -> Vec<u8> {
        vec![0x00; (self.config.screen_width * self.config.screen_height) as usize]
    }

    pub fn new(config: &DisplayConfig) -> PrintDisplay {
        PrintDisplay {
            frame_buffer: WrappedFramebuffer {
                frame_buffer: Framebuffer::new(config.frame_buffer.clone()).ok(),
                fb_path: config.frame_buffer.clone(),
            },
            config: config.clone(),
        }
    }
}

impl Clone for PrintDisplay {
    fn clone(&self) -> Self {
        Self::new(&self.config.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_re_encode_565() {
        // Input buffer of 3 bytes
        let image_buffer: Vec<u8> = vec![0xFF,0xFF,0xFF];
        let image_bit_depth = 8;

        // Re-encoded for 565 bit schema
        let display_bit_depths = vec![5,6,5];

        // Should output two bytes, corresponding to 11111 111111 11111
        let expected_result = vec![0xFF,0xFF];

        let result = PrintDisplay::_re_encode(image_buffer, image_bit_depth, &display_bit_depths);

        assert_eq!(result, expected_result);
    }

    #[test]
    fn test_re_encode_noop() {
        // Input buffer of 3 bytes
        let image_buffer: Vec<u8> = vec![0xFF,0xFF,0xFF];
        let image_bit_depth = 8;

        // no-op re-encoded for 8 bit display
        let display_bit_depths = vec![8];

        // Should output exactly what came in
        let expected_result = vec![0xFF,0xFF,0xFF];

        let result = PrintDisplay::_re_encode(image_buffer, image_bit_depth, &display_bit_depths);

        assert_eq!(result, expected_result);

    }
}
