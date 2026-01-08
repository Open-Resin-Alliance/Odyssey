use odyssey::configuration::{
    ApiConfig, Configuration, DisplayConfig, GcodeConfig, PixelFormat, PrinterConfig,
};

pub mod mock_serial_handler;

#[allow(unused_variables)]
pub static TEST_RESOURCE_DIR: &str = "tests/resources";
pub static RESOURCE_DIR: &str = "resources";
pub static UPLOAD_DIR: &str = "uploads";
pub static CARGO_DIR: &str = env!("CARGO_MANIFEST_DIR");

#[allow(dead_code)]
pub fn default_test_configuration() -> Configuration {
    Configuration {
        config_file: Some("".to_owned()),
        printer: PrinterConfig {
            serial: String::from("/dev/null"),
            baudrate: 250000,
            max_z: 300.0,
            default_lift: 10.0,
            default_up_speed: 3.4,
            default_down_speed: 3.4,
            default_wait_before_exposure: 2.2,
            default_wait_after_exposure: 1.5,
            pause_lift: 100.0,
        },
        gcode: GcodeConfig {
            boot: String::from("G90"),
            shutdown: String::from("M84\nUVLED_OFF"),
            home_command: String::from("HOME_AXIS"),
            move_command: String::from("MOVE_PLATE Z={z} F={speed}"),
            print_start: String::from("START_GCODE TOTAL_LAYERS={total_layers}"),
            print_end: String::from("END_GCODE"),
            layer_start: String::from("LAYER_START_GCODE LAYER={layer}"),
            cure_start: String::from("START_CURE"),
            cure_end: String::from("END_CURE"),
            move_sync: String::from("MOVE COMPLETE RESPONSE"),
            move_timeout: 60,
            status_check: String::from("STATUS_GCODE"),
            status_desired: String::from("READY STATUS RESPONSE"),
            manual_move_command: None,
        },
        api: ApiConfig {
            upload_path: upload_path(),
            usb_glob: upload_path(),
            port: 12357,
            enable_docs: Some(true),
        },
        display: DisplayConfig {
            frame_buffer: "/dev/null".to_owned(),
            screen_width: 1920,
            screen_height: 1080,
            pixel_format: PixelFormat {
                bit_depth: vec![5, 6, 5],
                left_pad_bits: 0,
                right_pad_bits: 0,
            },
        },
    }
}

#[allow(dead_code)]
pub fn resource_path(resource_file: String) -> String {
    format!("{CARGO_DIR}/{RESOURCE_DIR}/{resource_file}")
}

#[allow(dead_code)]
pub fn test_resource_path(resource_file: String) -> String {
    format!("{CARGO_DIR}/{TEST_RESOURCE_DIR}/{resource_file}")
}

#[allow(dead_code)]
pub fn upload_path() -> String {
    format!("{CARGO_DIR}/{UPLOAD_DIR}")
}
