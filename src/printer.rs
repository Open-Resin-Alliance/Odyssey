use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::api_objects::FileMetadata;
use crate::api_objects::PrintMetadata;
use crate::api_objects::PrinterState;
use crate::api_objects::PrinterStatus;
use crate::configuration::*;
use crate::display::*;
use crate::error::OdysseyError;
use crate::hardware_control::HardwareControl;
use crate::printfile::Layer;
use crate::printfile::PrintFile;
use crate::{api_objects::DisplayTest, hardware_control::HardwareState};
use tokio::time::{interval, sleep, Duration};

pub struct Printer<T: HardwareControl> {
    pub full_config: Arc<Configuration>,
    pub display: PrintDisplay,
    pub hardware_controller: T,
    pub operation_receiver: mpsc::Receiver<Operation>,
    pub cancellation_token: CancellationToken,
    state_sender: watch::Sender<PrinterState>,
    pub state_receiver: watch::Receiver<PrinterState>,
}

impl<T: HardwareControl> Printer<T> {
    pub fn new(
        full_config: Arc<Configuration>,
        display: PrintDisplay,
        hardware_controller: T,
        operation_receiver: mpsc::Receiver<Operation>,
        cancellation_token: CancellationToken,
    ) -> Self {
        let (state_sender, state_receiver) = watch::channel::<PrinterState>(Default::default());

        Printer {
            full_config: full_config.clone(),
            display,
            hardware_controller,
            operation_receiver,
            state_sender,
            state_receiver,
            cancellation_token,
        }
    }

    pub async fn start_printer(mut self) -> Result<(),OdysseyError> {
        self.hardware_controller
            .add_state_variable("max_z", self.full_config.printer.max_z.to_string()).await;
        self.hardware_controller
            .add_state_variable("z_lift", self.full_config.printer.default_lift.to_string()).await;

        self.start_statemachine().await
    }

    pub async fn print_event_loop(&mut self) -> Result<(), OdysseyError> {
        let file: Arc<RwLock<Box<dyn PrintFile + Send + Sync>>> = Arc::new(RwLock::new(
            self.get_print_metadata()
                .ok_or(OdysseyError::internal_state_error(
                    "Currently printing but no file data available".into(),
                    500,
                ))?
                .file_data
                .try_into()?,
        ));

        let layer_height = file.read().await.get_layer_height();

        // Get movement values from file, or configured defaults
        let lift = file
            .read()
            .await
            .get_lift()
            .unwrap_or((self.full_config.printer.default_lift * 1000.0).trunc() as u32);
        let up_speed = file
            .read()
            .await
            .get_up_speed()
            .unwrap_or(self.full_config.printer.default_up_speed);
        let down_speed = file
            .read()
            .await
            .get_down_speed()
            .unwrap_or(self.full_config.printer.default_down_speed);

        let wait_before_exposure = file
            .read()
            .await
            .get_wait_before_exposure()
            .unwrap_or(self.full_config.printer.default_wait_before_exposure);
        let wait_after_exposure = file
            .read()
            .await
            .get_wait_after_exposure()
            .unwrap_or(self.full_config.printer.default_wait_after_exposure);

        let mut pause_interv = interval(Duration::from_millis(100));

        self.hardware_controller
            .add_state_variable("total_layers", file.read().await.get_layer_count().to_string()).await;

        // Execute start_print command, then report state
        self.wrapped_start_print().await;

        // Fetch and generate the first frame
        let layer = file.write().await.get_layer_data(0).await;
        let mut optional_frame = Frame::from_layer(layer).await;

        loop {
            // Run any requested operations that may change the printer state
            self.printing_operation_handler().await;

            let state = self.state_receiver.borrow().clone();

            match state.status {
                PrinterStatus::Printing => {
                    let paused = state.paused.unwrap_or_default();
                    let layer = state.layer.unwrap_or_default();
                    if paused {
                        pause_interv.tick().await;
                        continue;
                    } else {
                        match optional_frame {
                            // More frames exist, continue printing
                            Some(cur_frame) => {
                                self.hardware_controller.add_state_variable("layer", layer.to_string()).await;
                                // Start a task to fetch and generate the next
                                // frame while we're exposing the current one
                                let gen_next_frame = tokio::spawn(Frame::from_layer(
                                    file.write().await.get_layer_data(layer + 1).await,
                                ));

                                // Print the current frame by moving into
                                // position and curing
                                self.print_frame(
                                    cur_frame,
                                    layer,
                                    layer_height,
                                    lift,
                                    up_speed,
                                    down_speed,
                                    wait_before_exposure,
                                    wait_after_exposure,
                                )
                                .await;

                                // Await generation of the next frame
                                optional_frame =
                                    gen_next_frame.await.expect("Layer generation task failed");

                                // Bump current layer
                                self.set_layer(layer + 1).await;
                            }
                            // No more frames remain, end print
                            None => self.end_print().await,
                        }
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    async fn print_frame(
        &mut self,
        cur_frame: Frame,
        layer: usize,
        layer_height: u32,
        lift: u32,
        up_speed: f64,
        down_speed: f64,
        wait_before_exposure: f64,
        wait_after_exposure: f64,
    ) {
        tracing::info!("Begin layer {}", layer);
        self.wrapped_start_layer(layer).await;
        let layer_z = ((layer + 1) as u32) * layer_height;
        //let lift_z = layer_z+

        let exposure_time = cur_frame.exposure_time;

        // Move the plate up first, then down into position
        tracing::info!("Moving to layer position {}", layer_z);

        self.wrapped_move(layer_z + lift, up_speed).await;
        self.wrapped_move(layer_z, down_speed).await;

        // Wait for configured time before curing
        tracing::info!("Waiting for {}s before cure", wait_before_exposure);
        sleep(Duration::from_secs_f64(wait_before_exposure)).await;

        // Display the current frame to the LCD
        tracing::info!("Loading layer to display");
        self.display.display_frame(cur_frame);

        // Activate the UV array for the prescribed length of time
        tracing::info!("Curing layer for {}s", exposure_time);
        self.wrapped_start_cure().await;
        sleep(Duration::from_secs_f64(exposure_time)).await;
        self.wrapped_stop_cure().await;

        // Wait for configured time after curing
        tracing::info!("Waiting for {}s after cure", wait_after_exposure);
        sleep(Duration::from_secs_f64(wait_after_exposure)).await;
    }

    async fn wrapped_start_print(&mut self) {
        if let Ok(hardware_state) = self.hardware_controller.start_print().await {
            self.update_hardware_state(hardware_state).await;
        } else {
            self.shutdown().await;
        }
    }

    async fn wrapped_start_layer(&mut self, layer: usize) {
        if let Ok(hardware_state) = self.hardware_controller.start_layer(layer).await {
            self.update_hardware_state(hardware_state).await;
        } else {
            self.shutdown().await;
        }
    }

    // Execute command and update printer state
    async fn wrapped_command(&mut self, command: String) {
        if let Ok(_command_response) = self.hardware_controller.manual_command(command).await {
            self.update_hardware_state(self.hardware_controller.get_hardware_state())
                .await;
        } else {
            self.shutdown().await;
        }
    }

    // Home and update printer state
    async fn wrapped_home(&mut self) {
        if let Ok(hardware_state) = self.hardware_controller.home().await {
            self.update_hardware_state(hardware_state).await;
        } else {
            self.shutdown().await;
        }
    }

    // Move and update printer state
    async fn wrapped_move(&mut self, z: u32, speed: f64) {
        self._wrapped_move(z, speed, false).await
    }
    async fn wrapped_manual_move(&mut self, z: u32, speed: f64) {
        self._wrapped_move(z, speed, true).await
    }
    async fn _wrapped_move(&mut self, z: u32, speed: f64, manual: bool) {
        if let Ok(hardware_state) = self.hardware_controller.move_z(z, speed, manual).await {
            self.update_hardware_state(hardware_state).await;
        } else {
            self.shutdown().await;
        }
    }

    // Start cure and update printer state
    async fn wrapped_start_cure(&mut self) {
        if let Ok(hardware_state) = self.hardware_controller.start_curing().await {
            self.update_hardware_state(hardware_state).await;
        } else {
            self.shutdown().await;
        }
    }

    // Stop cure and update printer state
    async fn wrapped_stop_cure(&mut self) {
        if let Ok(hardware_state) = self.hardware_controller.stop_curing().await {
            self.update_hardware_state(hardware_state).await;
        } else {
            self.shutdown().await;
        }
    }

    // Move only if paused
    async fn paused_move(&mut self, z: u32, speed: f64) {
        if self.state_receiver.borrow().paused.unwrap_or(false) {
            self.wrapped_manual_move(z.max(self._get_layer_z()), speed)
                .await;
        }
    }

    // Update layer in printer state
    async fn set_layer(&mut self, layer: usize) {
        self.update_layer(layer).await;
    }

    pub async fn start_print(&mut self, file_data: FileMetadata) -> Result<(), OdysseyError> {
        tracing::info!("Starting Print");

        let print_file: Box<dyn PrintFile + Send + Sync> = file_data.try_into()?;
        self.enter_printing_state(print_file.get_metadata()).await;
        Ok(())
    }

    async fn end_print(&mut self) {
        if let Ok(hardware_state) = self.hardware_controller.end_print().await {
            self.hardware_controller
                .remove_state_variable("total_layers").await;
            self.hardware_controller.remove_state_variable("layer").await;
            self.update_idle_state(hardware_state).await;
            tracing::info!("Print complete.");
        } else {
            self.shutdown().await;
        }
    }

    async fn pause_print(&mut self) {
        self.update_paused(true).await;
        let new_z_microns = ((self.full_config.printer.max_z * 1000.0).trunc() as u32).min(
            ((self.hardware_controller.get_hardware_state().z * 1000.0).trunc() as u32)
                + ((self.full_config.printer.pause_lift * 1000.0).trunc() as u32),
        );
        self.wrapped_move(new_z_microns, self.full_config.printer.default_up_speed)
            .await;
    }

    async fn resume_print(&mut self) {
        self.update_paused(false).await;
    }

    fn _get_layer(&self) -> usize {
        self.state_receiver.borrow().layer.unwrap_or(0)
    }

    fn _get_layer_z(&self) -> u32 {
        ((self._get_layer() + 1) as u32)
            * self
                .state_receiver
                .borrow()
                .print_data
                .clone()
                .map(|print| print.layer_height_microns)
                .unwrap_or(0)
    }

    fn get_print_metadata(&self) -> Option<PrintMetadata> {
        self.state_receiver.borrow().print_data.clone()
    }

    async fn display_file_layer(
        &mut self,
        file_data: FileMetadata,
        layer: usize,
    ) -> Result<(), OdysseyError> {
        tracing::info!("Loading layer {} from {} to display", layer, file_data.name);
        let mut file: Box<dyn PrintFile + Send + Sync> = file_data.try_into()?;

        let optional_frame = Frame::from_layer(file.get_layer_data(layer).await).await;

        if let Some(frame) = optional_frame {
            self.display.display_frame(frame);
        }
        Ok(())
    }

    async fn enter_printing_state(&mut self, print_data: PrintMetadata) {
        tracing::info!("Entering printing state");
        let status = self.state_receiver.borrow().status;
        match status {
            PrinterStatus::Idle => {
                tracing::debug!("Transitioning from Idle State");
                self.state_sender.send_modify(|state| {
                    state.print_data = Some(print_data);
                    state.paused = Some(false);
                    state.layer = Some(0);
                    state.status = PrinterStatus::Printing;
                });
            }
            PrinterStatus::Printing => {
                tracing::debug!("Already in printing state!");
            }
            PrinterStatus::Shutdown => {
                tracing::debug!("Cannot start print, Odyssey shutdown");
            }
        }
    }

    async fn update_hardware_state(&mut self, new_hardware_state: HardwareState) {
        self.state_sender
            .send_modify(|state| state.hardware_state = new_hardware_state);
    }

    async fn update_paused(&mut self, new_pause: bool) {
        if matches!(self.state_receiver.borrow().status, PrinterStatus::Printing) {
            self.state_sender
                .send_modify(|state| state.paused = Some(new_pause));
        }
    }

    async fn update_layer(&mut self, new_layer: usize) {
        if matches!(self.state_receiver.borrow().status, PrinterStatus::Printing) {
            self.state_sender
                .send_modify(|state| state.layer = Some(new_layer));
        }
    }

    async fn printing_operation_handler(&mut self) {
        /*if !self.verify_hardware().await {
            return;
        }*/

        let mut op_result = self.operation_receiver.try_recv();

        while let Ok(operation) = op_result {
            match operation {
                Operation::PausePrint => self.pause_print().await,
                Operation::ResumePrint => self.resume_print().await,
                Operation::StopPrint => self.set_idle().await,
                Operation::Shutdown => self.shutdown().await,
                Operation::ManualMove { z } => {
                    self.paused_move(z, self.full_config.printer.default_up_speed)
                        .await
                }
                _ => (),
            };
            op_result = self.operation_receiver.try_recv();
        }
    }

    pub async fn boot(&mut self) {
        tracing::info!("Booting up printer.");

        match self.hardware_controller.boot().await {
            Ok(hardware_state) => {
                self.update_idle_state(hardware_state).await;
            }
            Err(e) => {
                tracing::error!("Error booting printer:{}", e);
                self.shutdown().await;
            }
        }
    }

    pub async fn _verify_hardware(&mut self) -> bool {
        if let Ok(false) = self.hardware_controller.is_ready().await {
            tracing::error!("Hardware controller no longer ready! Shutting down Odyssey");
            self.shutdown().await;
            return false;
        }
        true
    }

    pub async fn shutdown(&mut self) {
        tracing::info!("Shutting down.");
        // If hardware still running, execute shutdown commands
        if let Ok(true) = self.hardware_controller.is_ready().await {
            if (self.hardware_controller.shutdown().await).is_ok() {
                tracing::info!("Shut down gcode executed successfully")
            } else {
                tracing::info!("Unable to execute shutdown gcode")
            }
        }

        self.cancellation_token.cancel();

        self.state_sender.send_modify(|state| {
            state.status = PrinterStatus::Shutdown;
            state.paused = None;
            state.hardware_state = self.hardware_controller.get_hardware_state()
        });
    }

    pub fn get_state_receiver(&self) -> watch::Receiver<PrinterState> {
        self.state_receiver.clone()
    }

    pub async fn start_statemachine(&mut self) -> Result<(), OdysseyError> {

        let mut interv = interval(Duration::from_millis(1000));

        loop {
            if self.cancellation_token.is_cancelled() {
                log::info!("Shutting down statemachine");
                self.hardware_controller.shutdown().await?;
                break;
            }
            let status = self.state_receiver.borrow().status;
            match status {
                PrinterStatus::Idle => self.idle_event_loop().await,
                PrinterStatus::Printing => self
                    .print_event_loop()
                    .await
                    .expect("Unexpected error during print"),
                PrinterStatus::Shutdown => self.shutdown_event_loop().await,
            }

            interv.tick().await;
        }
        Ok(())
    }

    async fn shutdown_event_loop(&mut self) {
        let mut shutdown_interv = interval(Duration::from_secs(10));

        self.shutdown_operation_handler().await;
        let status = self.state_receiver.borrow().status.clone();
        if matches!(status, PrinterStatus::Shutdown) {
            if self.hardware_controller.is_ready().await.unwrap_or(false) {
                self.boot().await;
            }
            else {
                shutdown_interv.tick().await;
            }
        }
    }

    // While in shutdown state, process operations to drop them from queue
    async fn shutdown_operation_handler(&mut self) {
        let mut op_result = self.operation_receiver.try_recv();

        while let Ok(operation) = op_result {
            tracing::warn!(
                "Received state machine request while shutdown:{:?}",
                operation
            );
            op_result = self.operation_receiver.try_recv();
        }
    }

    async fn set_idle(&mut self) {
        self.state_sender.send_modify(|state| {
            state.status = PrinterStatus::Idle;
            state.layer = None;
            state.paused = None;
        });
    }

    async fn update_idle_state(&mut self, hardware_state: HardwareState) {
        self.state_sender.send_modify(|state| {
            state.status = PrinterStatus::Idle;
            state.hardware_state = hardware_state;
        });
    }

    async fn idle_operation_handler(&mut self) {
        /*if !self.verify_hardware().await {
            return;
        }*/

        let mut op_result = self.operation_receiver.try_recv();

        while let Ok(operation) = op_result {
            match operation {
                Operation::StartPrint { file_data } => {
                    self.start_print(file_data).await.unwrap_or(())
                }
                Operation::ManualCommand { command } => self.wrapped_command(command).await,
                Operation::ManualHome => self.wrapped_home().await,
                Operation::ManualMove { z } => {
                    self.wrapped_manual_move(z, self.full_config.printer.default_up_speed)
                        .await
                }
                Operation::ManualCure { cure } => {
                    if cure {
                        self.wrapped_start_cure().await;
                    } else {
                        self.wrapped_stop_cure().await;
                    }
                }
                Operation::ManualDisplayTest { test } => {
                    self.display.display_test(test);
                }
                Operation::ManualDisplayLayer { file_data, layer } => {
                    self.display_file_layer(file_data, layer)
                        .await
                        .unwrap_or(());
                }
                Operation::Shutdown => self.shutdown().await,
                _ => (),
            };
            op_result = self.operation_receiver.try_recv();
        }
    }

    async fn idle_event_loop(&mut self) {
        self.idle_operation_handler().await;
    }
}

impl Frame {
    async fn from_layer(layer: Option<Layer>) -> Option<Frame> {
        if layer.is_some() {
            let layer = layer.unwrap();
            let frame = Frame::from_vec(layer.file_name, layer.exposure_time, layer.data);
            return Some(frame);
        }
        None
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    StartPrint {
        file_data: FileMetadata,
    },
    StopPrint,
    PausePrint,
    ResumePrint,
    ManualMove {
        z: u32,
    },
    ManualCure {
        cure: bool,
    },
    ManualHome,
    ManualCommand {
        command: String,
    },
    ManualDisplayLayer {
        file_data: FileMetadata,
        layer: usize,
    },
    ManualDisplayTest {
        test: DisplayTest,
    },
    QueryState,
    Shutdown,
}
