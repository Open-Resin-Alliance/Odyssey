use core::panic;
use std::collections::HashMap;

use async_trait::async_trait;
use regex::Regex;
use tokio::time::Duration;

use crate::api_objects::PhysicalState;
use crate::configuration::GcodeConfig;
use crate::error::OdysseyError;
use crate::printer::HardwareControl;
use crate::serial_handler::InternalCommsHandler;

pub struct Gcode {
    pub config: GcodeConfig,
    pub state: PhysicalState,
    pub gcode_substitutions: HashMap<String, String>,
    pub serial_comms: InternalCommsHandler,
}

impl Gcode {
    pub fn new(config: &GcodeConfig, serial_comms: InternalCommsHandler) -> Gcode {
        Gcode {
            config: config.clone(),
            state: PhysicalState {
                z: 0.0,
                z_microns: 0,
                curing: false,
            },
            gcode_substitutions: HashMap::new(),
            serial_comms,
        }
    }

    fn parse_gcode(&mut self, code: String) -> String {
        let re: Regex = Regex::new(r"\{(?P<substitution>\w*)\}").unwrap();
        let mut parsed_code = code.clone();

        self.add_state_variables();

        for caps in re.captures_iter(&code) {
            let sub = &caps["substitution"].to_string();
            if let Some(value) = self.gcode_substitutions.get(sub) {
                parsed_code = parsed_code.replace(&format!("{{{sub}}}"), value)
            } else {
                panic!("Attempted to use gcode substitution {} in context where it was unavailable: {}", sub, code);
            }
        }
        parsed_code
    }

    async fn send_gcode(&mut self, code: String) -> Result<(), OdysseyError> {
        let parsed_code = self.parse_gcode(code) + "\r\n";
        tracing::debug!("Executing gcode: {}", parsed_code.trim_end());

        self.serial_comms.send(parsed_code).await
    }

    async fn send_and_await_gcode(
        &mut self,
        code: String,
        expect: &String,
        timeout_seconds: u64,
    ) -> Result<(), OdysseyError> {
        let parsed_code = self.parse_gcode(code) + "\r\n";

        self.serial_comms
            .send_and_await(parsed_code, expect, Duration::from_secs(timeout_seconds))
            .await
    }

    async fn send_and_check_gcode(
        &mut self,
        code: String,
        expect: &String,
    ) -> Result<bool, OdysseyError> {
        let parsed_code = self.parse_gcode(code) + "\r\n";
        self.serial_comms.send_and_check(parsed_code, expect).await
    }

    /// Set the internally-stored position. Any method which uses a send_gcode
    /// method to cause the z axis to move, should call this method to reflect
    /// that change
    fn set_position(&mut self, position: u32) -> PhysicalState {
        self.state.z_microns = position;
        self.state.z = (position as f64) / 1000.0;
        self.state
    }

    /// Set the internally-stored curing state. Any method which uses a send_gcode
    /// method to enable or disable the LED array (or other curing method) should
    /// call this method to reflect that change
    fn set_curing(&mut self, curing: bool) -> PhysicalState {
        self.state.curing = curing;
        self.state
    }

    fn add_state_variables(&mut self) {
        self.gcode_substitutions
            .insert("curing".to_string(), self.state.curing.to_string());
        self.gcode_substitutions
            .insert("z".to_string(), self.state.z.to_string());
    }
}

#[async_trait]
impl HardwareControl for Gcode {
    async fn initialize(&mut self) {}

    async fn is_ready(&mut self) -> Result<bool, OdysseyError> {
        self.send_and_check_gcode(
            self.config.status_check.clone(),
            &self.config.status_desired.clone(),
        )
        .await
    }

    async fn home(&mut self) -> Result<PhysicalState, OdysseyError> {
        self.send_gcode(self.config.home_command.clone()).await?;

        Ok(self.state)
    }

    async fn manual_command(&mut self, command: String) -> Result<PhysicalState, OdysseyError> {
        self.send_gcode(command).await?;

        Ok(self.state)
    }

    async fn move_z(
        &mut self,
        z: u32,
        speed: f64,
        manual: bool,
    ) -> Result<PhysicalState, OdysseyError> {
        // Convert from mm/s to mm/min f value
        let speed = speed * 60.0;

        let command = match manual {
            true => match &self.config.manual_move_command {
                Some(manual_move) => manual_move.clone(),
                None => self.config.move_command.clone(),
            },
            false => self.config.move_command.clone(),
        };

        self.set_position(z);
        self.add_print_variable("speed".to_string(), speed.to_string());

        self.send_and_await_gcode(
            command,
            &self.config.move_sync.clone(),
            self.config.move_timeout,
        )
        .await?;

        self.remove_print_variable("speed".to_string());

        Ok(self.state)
    }

    async fn start_layer(&mut self, _layer: usize) -> Result<PhysicalState, OdysseyError> {
        self.send_gcode(self.config.layer_start.clone()).await?;

        Ok(self.state)
    }

    async fn start_curing(&mut self) -> Result<PhysicalState, OdysseyError> {
        self.set_curing(true);

        self.send_gcode(self.config.cure_start.clone()).await?;

        Ok(self.state)
    }

    async fn stop_curing(&mut self) -> Result<PhysicalState, OdysseyError> {
        self.set_curing(false);
        self.send_gcode(self.config.cure_end.clone()).await?;
        Ok(self.state)
    }

    async fn start_print(&mut self) -> Result<PhysicalState, OdysseyError> {
        self.send_gcode(self.config.print_start.clone()).await?;

        Ok(self.state)
    }

    async fn end_print(&mut self) -> Result<PhysicalState, OdysseyError> {
        self.send_gcode(self.config.print_end.clone()).await?;

        Ok(self.state)
    }

    async fn boot(&mut self) -> Result<PhysicalState, OdysseyError> {
        self.send_gcode(self.config.boot.clone()).await?;

        Ok(self.state)
    }

    async fn shutdown(&mut self) -> Result<(), OdysseyError> {
        self.send_gcode(self.config.shutdown.clone()).await?;

        Ok(())
    }

    fn get_physical_state(&self) -> Result<PhysicalState, OdysseyError> {
        Ok(self.state)
    }

    fn add_print_variable(&mut self, variable: String, value: String) {
        self.gcode_substitutions.insert(variable, value);
    }

    fn remove_print_variable(&mut self, variable: String) {
        self.gcode_substitutions.remove(&variable);
    }

    fn clear_variables(&mut self) {
        self.gcode_substitutions.clear();
    }
}
