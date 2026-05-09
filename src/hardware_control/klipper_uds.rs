use std::{collections::HashMap, fmt::Display, io, sync::Arc, time::Duration};

use async_trait::async_trait;
use poem_openapi::Enum;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{from_value, json, Value};
use tokio::{
    net::UnixStream,
    sync::{
        RwLock, oneshot::{self, error::RecvError}, watch
    },
    task::{self, JoinHandle},
    time::{interval, timeout},
};
use tokio_util::sync::CancellationToken;
use tracing::instrument;
use uuid::Uuid;

use crate::{
    configuration::KlipperUDSConfig,
    error::OdysseyError,
    hardware_control::{HardwareControl, HardwareState, HardwareStatusEnum},
};

const KLIPPER_STATE_SOURCE: &str = "klipper_state"; // params.status.webhooks.state
const KLIPPER_STATE_JSON_PATH: &str = "/webhooks/state";

const KLIPPER_STATE_MESSAGE_SOURCE: &str = "klipper_state_message"; // params.status.webhooks.state_message
const KLIPPER_STATE_MESSAGE_JSON_PATH: &str = "/webhooks/state_message";

const POSITION_SOURCE: &str = "gcode_position"; // params.status.gcode_move.gcode_position
const POSITION_JSON_PATH: &str = "/gcode_move/gcode_position/2";
const CURING_SOURCE: &str = "curing"; // ????

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KlipperResponseObject {
    pub id: Option<String>,
    pub source: Option<String>,
    pub result: Option<Value>,
    pub params: Option<Value>,
    #[serde(rename = "eventtime")]
    pub event_time: Option<f32>,
}
impl Display for KlipperResponseObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Enum)]
pub enum KlipperRequestMethod {
    #[serde(rename="emergency_stop")]
    EmergencyStop,

    #[serde(rename="objects/list")]
    ObjectsList,
    #[serde(rename="objects/query")]
    ObjectsQuery,
    #[serde(rename="objects/subscribe")]
    ObjectsSubscribe,
    
    #[serde(rename="gcode/script")]
    GcodeScript,
    #[serde(rename="gcode/restart")]
    GcodeRestart,
    #[serde(rename="gcode/firmware_restart")]
    GcodeFirmwareRestart,
    #[serde(rename="gcode/subscribe_output")]
    GcodeSubscribeOutput
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KlipperRequest {
    pub id: String,
    pub method: KlipperRequestMethod,
    pub params: Option<Value>,
}
impl KlipperRequest {
    fn emergency_stop(id: &str) -> KlipperRequest {
        KlipperRequest {
            id: id.to_string(),
            method: KlipperRequestMethod::EmergencyStop,
            params: None,
        }
    }

    fn objects_list(id: &str) -> KlipperRequest {
        KlipperRequest {
            id: id.to_string(),
            method: KlipperRequestMethod::ObjectsList,
            params: None,
        }
    }

    fn objects_query(id: &str, objects: HashMap<String, Option<Vec<String>>>) -> KlipperRequest {
        KlipperRequest {
            id: id.to_string(),
            method: KlipperRequestMethod::ObjectsQuery,
            params: Some(json!({
                "objects": objects
            })),
        }
    }

    fn objects_subscribe(
        id: &str,
        objects: &HashMap<&str, Option<Vec<&str>>>,
        source: &str,
    ) -> KlipperRequest {
        KlipperRequest {
            id: id.to_string(),
            method: KlipperRequestMethod::ObjectsSubscribe,
            params: Some(json!({
                "objects": objects,
                "response_template": {
                    "source": source
                }
            })),
        }
    }

    fn gcode_script(id: &str, script: &str) -> KlipperRequest {
        KlipperRequest {
            id: id.to_string(),
            method: KlipperRequestMethod::GcodeScript,
            params: Some(json!({
                "script": script
            })),
        }
    }

    fn gcode_restart(id: &str) -> KlipperRequest {
        KlipperRequest {
            id: id.to_string(),
            method: KlipperRequestMethod::GcodeRestart,
            params: None,
        }
    }

    fn gcode_firmware_restart(id: &str) -> KlipperRequest {
        KlipperRequest {
            id: id.to_string(),
            method: KlipperRequestMethod::GcodeFirmwareRestart,
            params: None,
        }
    }

    fn gcode_subscribe_output(id: &str) -> KlipperRequest {
        KlipperRequest {
            id: id.to_string(),
            method: KlipperRequestMethod::GcodeSubscribeOutput,
            params: Some(json!({
                "response_template": {
                    "source": "gcode"
                }
            })),
        }
    }
}

#[derive(Clone)]
pub struct KlipperUDS {
    pub config: KlipperUDSConfig,
    variable_substitutions: Arc<RwLock<HashMap<String, String>>>,
    param_watch_paths: Arc<RwLock<HashMap<String,String>>>,
    uds_stream: Option<Arc<UnixStream>>,
    response_send_channels: Arc<RwLock<HashMap<String, oneshot::Sender<Value>>>>,
    state_sender: watch::Sender<HardwareState>,
    pub state_receiver: watch::Receiver<HardwareState>,
}

impl KlipperUDS {
    pub fn new(config: &KlipperUDSConfig, uds_stream: Option<UnixStream>) -> KlipperUDS {
        let (state_sender, state_receiver) = watch::channel::<HardwareState>(Default::default());

        KlipperUDS {
            config: config.clone(),
            variable_substitutions: Arc::new(RwLock::new(HashMap::from([
                ("z".to_string(), "0.0".to_string()),
                ("curing".to_string(), "false".to_string()),
            ]))),
            param_watch_paths: Arc::new(RwLock::new(HashMap::from([
                (KLIPPER_STATE_SOURCE.to_string(),KLIPPER_STATE_JSON_PATH.to_string()),
                (KLIPPER_STATE_MESSAGE_SOURCE.to_string(),KLIPPER_STATE_MESSAGE_JSON_PATH.to_string()),
                (POSITION_SOURCE.to_string(),POSITION_JSON_PATH.to_string()),
            ]))),
            uds_stream: match uds_stream {
                Some(us) => Some(Arc::new(us)),
                None => None,
            },
            response_send_channels: Arc::new(RwLock::new(HashMap::new())),
            state_sender,
            state_receiver,
        }
    }

    pub fn get_uds(&self) -> Result<Arc<UnixStream>,io::Error>{
        self.uds_stream.clone().ok_or(io::Error::new(io::ErrorKind::NotConnected, "Not Connected to UDS"))
    }

    #[instrument(skip_all, level = "debug", ret)]
    pub async fn connect_uds(&mut self) -> Result<(),io::Error>{
        self.uds_stream = Some(Arc::new(UnixStream::connect(self.config.connection_path.clone()).await?));
        Ok(())
    }

    #[instrument(skip_all, level = "debug", fields(id = request.id))]
    pub async fn send_request(&self, request: &KlipperRequest) -> Result<(), io::Error> {
        let mut request_bytes = serde_json::to_vec(&request)?;
        request_bytes.push(0x03);
        timeout(Duration::from_secs(10), async {

            loop {
                self.get_uds()?.writable().await?;

                tracing::info!("sending request loop");
                match self
                    .get_uds()?
                    .try_write(&request_bytes)
                {
                    Ok(n) => {
                        tracing::debug!("wrote {} bytes to Klipper:\n{:#?}", n, request);
                        return Ok(());
                    }
                    Err(e) => match e.kind() {
                        std::io::ErrorKind::WouldBlock => {
                            continue;
                        }
                        _ => {
                            tracing::error!("Error encountered sending request to Klipper:\n{}", e);
                            return Err(e.into());
                        }
                    },
                }
            }
        }).await.map_err(|err| io::Error::new(io::ErrorKind::TimedOut, err)).flatten()

    }

    pub async fn send_request_with_response(
        &self,
        request: &KlipperRequest,
    ) -> Result<JoinHandle<Result<Value, RecvError>>, OdysseyError> {
        let (tx, rx) = oneshot::channel::<Value>();

        self.response_send_channels
            .write()
            .await
            .insert(request.id.to_owned(), tx);
        self.send_request(&request).await?;
        let response_handler = task::spawn(async move { rx.await });
        Ok(response_handler)
    }

    fn get_new_uuid(&self) -> String {
        Uuid::new_v4().to_string()
    }

    #[instrument(skip_all, level = "trace", ret)]
    fn parse_gcode(&self, code: &str) -> Result<String, OdysseyError> {
        let re: Regex = Regex::new(r"\{(?P<substitution>\w*)\}").unwrap();
        let mut parsed_code = code.to_owned();

        for caps in re.captures_iter(&code) {
            let sub = &caps["substitution"].to_string();
            if let Some(value) = self.variable_substitutions.blocking_read().get(sub) {
                parsed_code = parsed_code.replace(&format!("{{{sub}}}"), value)
            } else {
                return Err(
                    OdysseyError::internal_state_error(format!("Attempted to use gcode substitution {} in context where it was unavailable: {}", sub, code).into(), 500));
            }
        }
        Ok(parsed_code)
    }

    async fn add_state_variables(&mut self) {
        let mut var_subs = self.variable_substitutions.write().await;
        var_subs.insert(
            "curing".to_string(),
            self.state_receiver.borrow().curing.to_string(),
        );
        var_subs.insert("z".to_string(), self.state_receiver.borrow().z.to_string());
    }


    async fn handle_state_watch(&mut self, payload: &Value, source: &str) -> Result<(),OdysseyError>{

        // Get the watched path for this source, or return early
        let target_path = match self.param_watch_paths.read().await.get(source) {
            Some(path) => path.clone(),
            None => return Ok(()),
        };

        let target_value = match payload.pointer(&target_path) {
            Some(val) => val.clone(),
            None => return Ok(()),
        };

        match source {
            KLIPPER_STATE_SOURCE => {
                if let Some(extracted_payload_value) = from_value::<HardwareStatusEnum>(target_value).ok()
                {
                    self.state_sender.send_modify(|state| {
                        state.status = extracted_payload_value
                    });
                }
            }
            KLIPPER_STATE_MESSAGE_SOURCE => {
                if let Some(extracted_payload_value) = from_value::<String>(target_value).ok()
                {
                    self.state_sender.send_modify(|state| {
                        state.status_message = extracted_payload_value
                    });
                }
            }
            POSITION_SOURCE => {
                if let Some(extracted_payload_value) = from_value::<f64>(target_value).ok()
                {
                    self.state_sender.send_modify(|state| {
                        state.z = extracted_payload_value
                    });
                }
            }
            peripheral_source => {
                self.state_sender.send_modify(|state| {
                    state.peripheral.insert(peripheral_source.to_string(), target_value.to_string());
                });
            }
        }
        Ok(())
    }

    async fn emit_response(&mut self, payload: &Value, id: &str) -> Result<(),OdysseyError>{
        if let Some(response_channel) =
            self.response_send_channels.write().await.remove(id)
        {
            tracing::trace!("Emmitting response for request {}", id);
            let _ = response_channel.send(payload.clone());
        } else {
            tracing::trace!("No channel registered for request {}--not emitting response", id);
        }
        Ok(())
    }
}

#[async_trait]
impl HardwareControl for KlipperUDS {

    
    #[instrument(skip_all, level = "debug", ret)]
    async fn is_ready(&mut self) -> Result<bool, OdysseyError> {
        Ok(!matches!(self.get_hardware_state_receiver().borrow().status, HardwareStatusEnum::Startup))
    }

    
    #[instrument(skip_all, level = "debug", ret)]
    async fn initialize(&mut self) -> Result<(), OdysseyError> {
        if self.uds_stream.is_none() {
            self.connect_uds().await?;
        }

        self.send_request(&KlipperRequest::objects_subscribe(
            KLIPPER_STATE_SOURCE,
            &HashMap::from([("webhooks", Some(vec!["state"]))]),
            KLIPPER_STATE_SOURCE,
        ))
        .await?;

        self.send_request(&KlipperRequest::objects_subscribe(
            KLIPPER_STATE_MESSAGE_SOURCE,
            &HashMap::from([("webhooks", Some(vec!["state_message"]))]),
            KLIPPER_STATE_MESSAGE_SOURCE,
        ))
        .await?;

        self.send_request(&KlipperRequest::objects_subscribe(
            POSITION_SOURCE,
            &HashMap::from([("gcode_move", Some(vec!["gcode_position"]))]),
            POSITION_SOURCE,
        ))
        .await?;

        Ok(())
    }
    async fn home(&mut self) -> Result<HardwareState, OdysseyError> {
        self.send_request_with_response(&KlipperRequest::gcode_script(
            &self.get_new_uuid(),
            &self.parse_gcode(&self.config.home_command)?,
        ))
        .await?
        .await
        .unwrap()?;

        Ok(self.get_hardware_state())
    }
    async fn manual_command(&mut self, command: String) -> Result<String, OdysseyError> {
        Ok(self
            .send_request_with_response(&KlipperRequest::gcode_script(
                &self.get_new_uuid(),
                &command,
            ))
            .await?
            .await
            .unwrap()?
            .to_string())
    }
    async fn start_print(&mut self) -> Result<HardwareState, OdysseyError> {
        self.send_request_with_response(&KlipperRequest::gcode_script(
            &self.get_new_uuid(),
            &self.parse_gcode(&self.config.print_start)?,
        ))
        .await?
        .await
        .unwrap()?;

        Ok(self.get_hardware_state())
    }
    async fn end_print(&mut self) -> Result<HardwareState, OdysseyError> {
        self.send_request_with_response(&KlipperRequest::gcode_script(
            &self.get_new_uuid(),
            &self.parse_gcode(&self.config.print_end)?,
        ))
        .await?
        .await
        .unwrap()?;

        Ok(self.get_hardware_state())
    }
    async fn move_z(
        &mut self,
        z_microns: u32,
        speed: f64,
        manual: bool,
    ) -> Result<HardwareState, OdysseyError> {
        let z = (z_microns as f64) / 1000.0;
        // Convert from mm/s to mm/min f value
        let speed = speed * 60.0;
        self.add_state_variable("speed", speed.to_string()).await;
        self.add_state_variable("z", z.to_string()).await;

        self.send_request_with_response(&KlipperRequest::gcode_script(
            &self.get_new_uuid(),
            &self.parse_gcode(match manual {
                true => match &self.config.manual_move_command {
                    Some(manual_move) => manual_move,
                    None => &self.config.move_command,
                },
                false => &self.config.move_command,
            })?,
        ))
        .await?
        .await
        .unwrap()?;

        Ok(self.get_hardware_state())
    }
    async fn start_layer(&mut self, layer: usize) -> Result<HardwareState, OdysseyError> {
        self.add_state_variable("layer", layer.to_string()).await;
        self.send_request_with_response(&KlipperRequest::gcode_script(
            &self.get_new_uuid(),
            &self.parse_gcode(&self.config.layer_start)?,
        ))
        .await?
        .await
        .unwrap()?;

        Ok(self.get_hardware_state())
    }
    async fn start_curing(&mut self) -> Result<HardwareState, OdysseyError> {
        self.send_request_with_response(&KlipperRequest::gcode_script(
            &self.get_new_uuid(),
            &self.parse_gcode(&self.config.cure_start)?,
        ))
        .await?
        .await
        .unwrap()?;

        Ok(self.get_hardware_state())
    }
    async fn stop_curing(&mut self) -> Result<HardwareState, OdysseyError> {
        self.send_request_with_response(&KlipperRequest::gcode_script(
            &self.get_new_uuid(),
            &self.parse_gcode(&self.config.cure_end)?,
        ))
        .await?
        .await
        .unwrap()?;

        Ok(self.get_hardware_state())
    }
    async fn boot(&mut self) -> Result<HardwareState, OdysseyError> {
        self.send_request_with_response(&KlipperRequest::gcode_script(
            &self.get_new_uuid(),
            &self.parse_gcode(&self.config.boot)?,
        ))
        .await?
        .await
        .unwrap()?;

        Ok(self.get_hardware_state())
    }
    async fn shutdown(&mut self) -> Result<(), OdysseyError> {
        self.send_request_with_response(&KlipperRequest::gcode_script(
            &self.get_new_uuid(),
            &self.parse_gcode(&self.config.shutdown)?,
        ))
        .await?
        .await
        .unwrap()?;
        Ok(())
    }
    fn get_hardware_state(&self) -> HardwareState {
        self.state_receiver.borrow().clone()
    }
    fn get_hardware_state_receiver(&self) -> watch::Receiver<HardwareState> {
        self.state_receiver.clone()
    }
    async fn add_state_variable(&mut self, variable: &str, value: String) {
        self.variable_substitutions
            .write().await
            .insert(variable.to_string(), value);
    }
    async fn remove_state_variable(&mut self, variable: &str) {
        self.variable_substitutions
            .write().await
            .remove(variable);
    }
    async fn clear_state_variables(&mut self) {
        self.variable_substitutions.write().await.clear();
    }
    async fn run(mut self, cancellation_token: CancellationToken) -> Result<(), OdysseyError> {
        let mut interval = interval(Duration::from_millis(100));
        let mut init = false;

        let mut read_buf: [u8; 1024] = [0;1024];

        loop {
            if cancellation_token.is_cancelled() {
                tracing::info!("Shutting down KlipperUDS HardwareController");
                return self.shutdown().await;
            }

            // Initialize UDS connection and Klipper subscriptions
            if !init {
                self.initialize().await?;
                init = true;
            }

            cancellation_token.run_until_cancelled(self.get_uds()?.readable()).await;
            //self.get_uds()?.readable().await?;

            
            let data_read = match self.get_uds()?.try_read(&mut read_buf) {
                Ok(0) => {

                    tracing::info!("Read 0 bytes from Klipper, indicating a closed connection.");
                    init = false;
                    self.uds_stream = None;
                    continue;
                }
                Ok(n) => {
                    tracing::debug!("Read {} bytes from Klipper", n);
                    
                    &read_buf[0..n]
                }
                Err(e) => match e.kind() {
                    std::io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    _ => {
                        tracing::error!("Error encountered reading messages from Klipper:\n{}", e);
                        return Err(e.into());
                    }
                },
            };

            
            let split_reqs = data_read.split(|&byte| byte == 0x03).filter(|&data_piece| !data_piece.is_empty());

            for req_data in split_reqs {
                let klipper_resp_val = match serde_json::from_slice::<KlipperResponseObject>(req_data) {
                    Ok(val) => {
                        val
                    }
                    Err(err) => {
                        tracing::debug!(
                            "Encountered Error while parsing Klipper Response: {}",
                            err
                        );
                        if let Ok(response_string) = serde_json::from_slice::<String>(req_data) {
                            tracing::debug!("Raw response string: {:?}", response_string);
                        }
                        else {
                            tracing::trace!("Unparsed Data: {:?}", req_data);
                        }
                        continue;
                    }
                };

                tracing::debug!("Read {:?} from Klipper", klipper_resp_val);

                // response_params entries (such as source) are only available on subscription messages with params,
                // And will not appear in the result messages of the initial subscription, despite the presence of other relevant payload fields
                if let Some(source_or_id) = klipper_resp_val.source.or(klipper_resp_val.id) {
                    let params_or_result = klipper_resp_val.params.or(klipper_resp_val.result).unwrap_or(Value::Null);

                    self.handle_state_watch(&params_or_result, &source_or_id).await?;
                    self.emit_response(&params_or_result, &source_or_id).await?
                }
            }

            
            

            interval.tick().await;
        }
    }
}
