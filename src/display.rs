
use framebuffer::Framebuffer;
use png::Decoder;

use crate::{
    api_objects::DisplayTest,
    configuration::{DisplayConfig, PixelFormat},
    wrapped_framebuffer::WrappedFramebuffer,
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
    fn re_encode_pixel_group(
        pixel_format: &PixelFormat,
        pixels: &[u8],
        bit_depth: u8,
        chunk_size: u8,
    ) -> Vec<u8> {
        let mut raw_chunk = 0b0;
        let mut chunk_bytes: Vec<u8> = Vec::new();
        let mut shift = chunk_size - pixel_format.left_pad_bits;
        for i in 0..pixels.len() {
            shift -= pixel_format.bit_depth[i];

            // Truncate the pixel data to the displays bit depth, then shift it into place in the raw chunk bits
            raw_chunk |= ((pixels[i] as u64) >> (bit_depth - pixel_format.bit_depth[i])) << shift
        }

        //println!("{:#032b}", raw_chunk);

        for i in 0..(chunk_size / 8) {
            // pull the raw chunk back apart into bytes, for push into the new buffer
            let byte = ((raw_chunk >> (8 * i)) & 0xFF) as u8;
            chunk_bytes.push(byte);
        }

        chunk_bytes
    }
    fn re_encode(&self, buffer: Vec<u8>, bit_depth: u8) -> Vec<u8> {
        if self.config.pixel_format.bit_depth.len() == 1
            && self.config.pixel_format.bit_depth[0] == bit_depth
        {
            return buffer;
        }

        let chunk_size: u8 = self.config.pixel_format.left_pad_bits
            + self.config.pixel_format.bit_depth.iter().sum::<u8>()
            + self.config.pixel_format.right_pad_bits;
        tracing::debug!("Re-encoding frame with bit-depth {} into {} pixels in {} bits, with the following bit layout: {:?}", bit_depth, self.config.pixel_format.bit_depth.len(), chunk_size, self.config.pixel_format.bit_depth);

        buffer
            .chunks_exact(self.config.pixel_format.bit_depth.len())
            .flat_map(|pixel_group| {
                Self::re_encode_pixel_group(
                    &self.config.pixel_format,
                    pixel_group,
                    bit_depth,
                    chunk_size,
                )
            })
            .collect()
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
            DisplayTest::Diagonal => self.display_test_diagonal(16),
            DisplayTest::ValueRange => self.display_test_value_range(),
            DisplayTest::Grid => self.display_test_blank(),
            DisplayTest::Dimensions => self.display_test_blank(),
        };

        self.display_bytes(test_bytes, 8);
    }

    fn display_test_white(&mut self) -> Vec<u8> {
        vec![0xFF; (self.config.screen_width * self.config.screen_height) as usize]
    }

    fn display_test_blank(&mut self) -> Vec<u8> {
        vec![0x00; (self.config.screen_width * self.config.screen_height) as usize]
    }

    fn display_test_diagonal(&mut self, width: u32) -> Vec<u8> {
        let val_from_pixel_index = |index| {
            let row = index / self.config.screen_width;
            match ((index + row) / width) % 2 == 0 {
                true => 0x00,
                false => 0xFF,
            }
        };

        let pixel_count = self.config.screen_width * self.config.screen_height;
        (0..pixel_count).map(val_from_pixel_index).collect()
    }

    fn display_test_value_range(&mut self) -> Vec<u8> {
        let min_bit_depth = self
            .config
            .pixel_format
            .bit_depth
            .iter()
            .min()
            .cloned()
            .unwrap_or(8);
        let max_val = ((0b1 << min_bit_depth) - 1);
        let values: Vec<u8> = (0x00..max_val).collect();
        let block_width = self.config.screen_width / (max_val as u32);

        let val_from_pixel_index = |index| {
            let col = index % self.config.screen_width;
            let val = values[(col / block_width) as usize];
            tracing::info!("index {index} col {col} val {:X}|{:b}", val, val);
            val
        };

        let pixel_count = self.config.screen_width * self.config.screen_height;
        (0..pixel_count).map(val_from_pixel_index).collect()
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
        // Input buffer of 3 1-byte pixels
        let image_buffer: [u8; 3] = [0xFF, 0xFF, 0xFF];
        let image_bit_depth = 8;

        let chunk_size = 16;

        // Re-encoded for 565 bit schema
        let pixel_format = PixelFormat {
            bit_depth: vec![5, 6, 5],
            left_pad_bits: 0,
            right_pad_bits: 0,
        };

        // Should output two bytes, corresponding to 11111 111111 11111
        let expected_result = vec![0xFF, 0xFF];

        let result = PrintDisplay::re_encode_pixel_group(
            &pixel_format,
            &image_buffer,
            image_bit_depth,
            chunk_size,
        );

        assert_eq!(result, expected_result);
    }

    #[test]
    fn test_re_encode_3bit8() {
        // Input buffer of 8 1-byte pixels
        let image_buffer: [u8; 8] = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let image_bit_depth = 8;

        let chunk_size = 32;

        // Re-encoded for 16k bit schema
        let pixel_format = PixelFormat {
            bit_depth: vec![3, 3, 3, 3, 3, 3, 3, 3],
            left_pad_bits: 0,
            right_pad_bits: 8,
        };

        // Should output four bytes, corresponding to values of 7,7,7,7,7,7,7,7,<PADDING>
        let expected_result = vec![0x00, 0xFF, 0xFF, 0xFF];

        let result = PrintDisplay::re_encode_pixel_group(
            &pixel_format,
            &image_buffer,
            image_bit_depth,
            chunk_size,
        );

        assert_eq!(result, expected_result);
    }

    #[test]
    fn test_re_encode_noop() {
        // Input buffer of 1 1-byte pixel
        let image_buffer: [u8; 1] = [0xFF];
        let image_bit_depth = 8;

        let chunk_size = 8;

        // Re-encoded for 565 bit schema
        let pixel_format = PixelFormat {
            bit_depth: vec![8],
            left_pad_bits: 0,
            right_pad_bits: 0,
        };

        // Should output the same as what was input
        let expected_result = vec![0xFF];

        let result = PrintDisplay::re_encode_pixel_group(
            &pixel_format,
            &image_buffer,
            image_bit_depth,
            chunk_size,
        );

        assert_eq!(result, expected_result);
    }
}
