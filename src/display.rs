use std::cmp::max;

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
        for (i, pixel) in pixels.iter().enumerate() {
            shift -= pixel_format.bit_depth[i];

            // Truncate the pixel data to the displays bit depth, then shift it into place in the raw chunk bits
            raw_chunk |= ((*pixel as u64) >> (bit_depth - pixel_format.bit_depth[i])) << shift
        }

        let byte_order: Vec<u8> = if pixel_format.invert_byte_order {
            (0..(chunk_size / 8)).collect()
        } else {
            (0..(chunk_size / 8)).rev().collect()
        };

        for i in byte_order {
            // pull the raw chunk back apart into bytes, for push into the new buffer
            let byte = ((raw_chunk >> (8 * i)) & 0xFF) as u8;
            chunk_bytes.push(byte);
        }

        chunk_bytes
    }
    fn re_encode(pixel_format: &PixelFormat, buffer: Vec<u8>, bit_depth: u8) -> Vec<u8> {
        if pixel_format.bit_depth.len() == 1 && pixel_format.bit_depth[0] == bit_depth {
            return buffer;
        }

        let chunk_size: u8 = pixel_format.left_pad_bits
            + pixel_format.bit_depth.iter().sum::<u8>()
            + pixel_format.right_pad_bits;

        tracing::debug!(
            "Re-encoding frame with bit-depth {} into {} pixels in {} bits, with the following bit layout: {:?}",
            bit_depth,
            pixel_format.bit_depth.len(),
            chunk_size,
            pixel_format.bit_depth
        );

        buffer
            .chunks_exact(pixel_format.bit_depth.len())
            .flat_map(|pixel_group| {
                Self::re_encode_pixel_group(pixel_format, pixel_group, bit_depth, chunk_size)
            })
            .collect()
    }

    pub fn display_frame(&mut self, frame: Frame) {
        self.display_rencoded_bytes(frame.buffer, frame.bit_depth);
    }

    fn display_rencoded_bytes(&mut self, buffer: Vec<u8>, bit_depth: u8) {
        self.display_bytes(&Self::re_encode(
            &self.config.pixel_format,
            buffer,
            bit_depth,
        ));
    }
    fn display_bytes(&mut self, buffer: &[u8]) {
        self.frame_buffer.write_frame(buffer);
    }

    pub fn display_test(&mut self, test: DisplayTest, pixel_format: Option<&PixelFormat>) {
        let test_bytes = match test {
            DisplayTest::White => {
                Self::display_test_white(self.config.screen_width, self.config.screen_height)
            }
            DisplayTest::Blank => {
                Self::display_test_blank(self.config.screen_width, self.config.screen_height)
            }
            DisplayTest::Diagonal => {
                Self::display_test_diagonal(self.config.screen_width, self.config.screen_height, 32)
            }
            DisplayTest::ValueRange => Self::display_test_value_range(
                self.config.screen_width,
                self.config.screen_height,
                pixel_format.unwrap_or(&self.config.pixel_format),
            ),
            DisplayTest::Grid => {
                Self::display_test_blank(self.config.screen_width, self.config.screen_height)
            }
            DisplayTest::Dimensions => {
                Self::display_test_blank(self.config.screen_width, self.config.screen_height)
            }
        };

        self.display_bytes(&Self::re_encode(
            pixel_format.unwrap_or(&self.config.pixel_format),
            test_bytes,
            8,
        ));
    }

    fn display_test_white(display_width: u32, display_height: u32) -> Vec<u8> {
        vec![0xFF; (display_width * display_height) as usize]
    }

    fn display_test_blank(display_width: u32, display_height: u32) -> Vec<u8> {
        vec![0x00; (display_width * display_height) as usize]
    }

    fn display_test_diagonal(display_width: u32, display_height: u32, columns: u32) -> Vec<u8> {
        let col_width = display_width / columns;

        (0..display_height)
            .flat_map(|row| {
                (0..display_width)
                    .map(|col| 0xFF * (((col + row) / col_width) % 2) as u8)
                    .collect::<Vec<u8>>()
            })
            .collect()
    }

    fn display_test_value_range(
        display_width: u32,
        display_height: u32,
        pixel_format: &PixelFormat,
    ) -> Vec<u8> {
        let min_bit_depth = pixel_format.bit_depth.iter().min().cloned().unwrap_or(8);

        let num_vals = 2_u32.pow(min_bit_depth as u32);

        let block_width = max(display_width / num_vals, 1);
        tracing::debug!(
            "Dividing screen ({} pixels wide) into {} columns of {} pixels",
            display_width,
            num_vals,
            block_width
        );

        (0..display_height)
            .flat_map(|_|
            // since values are truncated during conversion to the desired
            // bit depth, we shift the values left into the appropriate positions
            (0..num_vals).rev().flat_map(|val|
                vec![(val<<(8-min_bit_depth)) as u8; block_width as usize]
        ))
            .collect()
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
        let image_buffer: [u8; 3] = [0xFF, 0x00, 0xFF];
        let image_bit_depth = 8;

        let chunk_size = 16;

        // Re-encoded for 565 bit schema
        let pixel_format = PixelFormat {
            bit_depth: vec![5, 6, 5],
            left_pad_bits: 0,
            right_pad_bits: 0,
            invert_byte_order: true,
        };

        // Should output two bytes, corresponding to 11111 000000 1111, but swapped
        let expected_result = vec![0x1F, 0xF8];

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
        let image_buffer: [u8; 8] = [0xFF, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let image_bit_depth = 8;

        let chunk_size = 32;

        // Re-encoded for 16k bit schema
        let pixel_format = PixelFormat {
            bit_depth: vec![3, 3, 3, 3, 3, 3, 3, 3],
            left_pad_bits: 0,
            right_pad_bits: 8,
            invert_byte_order: false,
        };

        // Should output four bytes, corresponding to values of 7,0,7,7,7,7,7,7,<PADDING>
        let expected_result = vec![0xE3, 0xFF, 0xFF, 0x00];

        let result = PrintDisplay::re_encode_pixel_group(
            &pixel_format,
            &image_buffer,
            image_bit_depth,
            chunk_size,
        );

        assert_eq!(result, expected_result);
    }

    #[test]
    fn test_re_encode_gray() {
        let image_bit_depth = 8;
        let chunk_size = 8;

        // Re-encoded for 16k bit schema
        let pixel_format = PixelFormat {
            bit_depth: vec![4, 4],
            left_pad_bits: 0,
            right_pad_bits: 0,
            invert_byte_order: false,
        };

        assert_eq!(
            vec![0xFF],
            PrintDisplay::re_encode_pixel_group(
                &pixel_format,
                &[0xFF, 0xFF],
                image_bit_depth,
                chunk_size,
            )
        );

        assert_eq!(
            vec![0x00],
            PrintDisplay::re_encode_pixel_group(
                &pixel_format,
                &[0x00, 0x00],
                image_bit_depth,
                chunk_size,
            )
        );

        assert_eq!(
            vec![0xEE],
            PrintDisplay::re_encode_pixel_group(
                &pixel_format,
                &[0xEF, 0xEF],
                image_bit_depth,
                chunk_size,
            )
        );

        assert_eq!(
            vec![0xEE],
            PrintDisplay::re_encode_pixel_group(
                &pixel_format,
                &[0xE0, 0xE0],
                image_bit_depth,
                chunk_size,
            )
        );
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
            invert_byte_order: false,
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

    #[test]
    fn test_blank_display() {
        let display_width: u32 = 4;
        let display_height: u32 = 3;

        #[rustfmt::skip]
        let expected_result: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ];

        let result = PrintDisplay::display_test_blank(display_width, display_height);
        assert_eq!(result, expected_result);
    }

    #[test]
    fn test_white_display() {
        let display_width: u32 = 4;
        let display_height: u32 = 3;

        #[rustfmt::skip]
        let expected_result: Vec<u8> = vec![
            0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF,
        ];

        let result = PrintDisplay::display_test_white(display_width, display_height);
        assert_eq!(result, expected_result);
    }

    #[test]
    fn test_diagonal_display() {
        let display_width: u32 = 6;
        let display_height: u32 = 3;
        let columns: u32 = 3;

        #[rustfmt::skip]
        let expected_result: Vec<u8> = vec![
            0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00,
            0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF,
            0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF,
        ];

        let result = PrintDisplay::display_test_diagonal(display_width, display_height, columns);
        assert_eq!(result, expected_result);
    }

    #[test]
    fn test_value_display() {
        let display_width: u32 = 8;
        let display_height: u32 = 3;
        let pixel_format: PixelFormat = PixelFormat {
            bit_depth: vec![2],
            left_pad_bits: 0,
            right_pad_bits: 0,
            invert_byte_order: false,
        };

        #[rustfmt::skip]
        let expected_result: Vec<u8> = vec![
            0xC0, 0xC0, 0x80, 0x80, 0x40, 0x40, 0x00, 0x00,
            0xC0, 0xC0, 0x80, 0x80, 0x40, 0x40, 0x00, 0x00,
            0xC0, 0xC0, 0x80, 0x80, 0x40, 0x40, 0x00, 0x00,
        ];

        let result =
            PrintDisplay::display_test_value_range(display_width, display_height, &pixel_format);
        assert_eq!(result, expected_result);
    }
}
