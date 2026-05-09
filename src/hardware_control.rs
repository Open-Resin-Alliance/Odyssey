use std::collections::HashMap;

use crate::error::OdysseyError;
use async_trait::async_trait;
use poem_openapi::{Enum, Object};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

//pub mod gcode;
pub mod klipper_uds;

#[derive(Clone, Debug, Serialize, Deserialize, Enum, Default)]
#[serde(rename_all = "lowercase")]
pub enum HardwareStatusEnum {
    Ready,
    #[default]
    Startup,
    Shutdown,
    Error,
}
#[derive(Clone, Debug, Serialize, Deserialize, Object, Default)]
pub struct HardwareState {
    pub status: HardwareStatusEnum,
    pub status_message: String,
    pub z: f64,
    pub curing: bool,
    pub peripheral: HashMap<String, String>,
}

#[async_trait]
pub trait HardwareControl {
    async fn is_ready(&mut self) -> Result<bool, OdysseyError>;
    async fn initialize(&mut self) -> Result<(), OdysseyError>;
    async fn home(&mut self) -> Result<HardwareState, OdysseyError>;
    async fn manual_command(&mut self, command: String) -> Result<String, OdysseyError>;
    async fn start_print(&mut self) -> Result<HardwareState, OdysseyError>;
    async fn end_print(&mut self) -> Result<HardwareState, OdysseyError>;
    async fn move_z(
        &mut self,
        z_microns: u32,
        speed: f64,
        manual: bool,
    ) -> Result<HardwareState, OdysseyError>;
    async fn start_layer(&mut self, layer: usize) -> Result<HardwareState, OdysseyError>;
    async fn start_curing(&mut self) -> Result<HardwareState, OdysseyError>;
    async fn stop_curing(&mut self) -> Result<HardwareState, OdysseyError>;
    async fn boot(&mut self) -> Result<HardwareState, OdysseyError>;
    async fn shutdown(&mut self) -> Result<(), OdysseyError>;
    fn get_hardware_state(&self) -> HardwareState;
    fn get_hardware_state_receiver(&self) -> watch::Receiver<HardwareState>;
    // TODO: Implement variables as streams, using tokio::watchStream for configured interaction variables,
    // and basic repeat/repeat_with streams for simpler variables. Allows single access pattern to cover
    // all of our bases
    async fn add_state_variable(&mut self, variable: &str, value: String);
    async fn remove_state_variable(&mut self, variable: &str);
    async fn clear_state_variables(&mut self);
    async fn run(self, cancellation_token: CancellationToken) -> Result<(), OdysseyError>;
}
