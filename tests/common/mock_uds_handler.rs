use std::{collections::HashMap, io, time::Duration};

use odyssey::{error::OdysseyError, hardware_control::{HardwareState, HardwareStatusEnum, klipper_uds::{KlipperRequest, KlipperRequestMethod, KlipperResponseObject}}, shutdown_handler::{self, ShutdownHandler}};
use serde_json::{Value, json};
use tokio::{io::AsyncWriteExt, net::UnixStream, time::{Interval, interval}};
use tokio_util::sync::CancellationToken;
use tracing::instrument;


pub struct MockHardwareUDS {
    pub unix_stream: UnixStream,
    pub mock_state: HardwareState
}

impl MockHardwareUDS {


    #[instrument(skip_all, level = "debug", fields(id = response.id))]
    async fn send_response_retry(&mut self, response: KlipperResponseObject,  retry_interval: &mut Interval,  option_max_retries: Option<u8>) -> Result<usize, io::Error> {
        let attempts: u8 = 0;
            
        let mut response_bytes = serde_json::to_vec(&response).unwrap();
        response_bytes.push(0x03);

        loop {

            if option_max_retries.map(|max_retries| attempts>=max_retries).unwrap_or(false)  {
                break Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("Failed to send data over UDS after {} tries", attempts)
                ));
            }

            self.unix_stream.writable().await?;


            match self
                .unix_stream
                .try_write(&response_bytes)
            {
                Ok(n) => {
                    tracing::debug!("wrote {} bytes to Odyssey:\n{:#?}", n, response);
                    break Ok(n);
                }
                Err(e) => match e.kind() {
                    std::io::ErrorKind::WouldBlock => {
                        retry_interval.tick().await;
                        continue;
                    }
                    _ => {
                        tracing::error!("Error encountered sending response to Odyssey:\n{}", e);
                        break Err(e.into());
                    }
                },
            }
        }
    }

    pub fn object_query_response(&self, objects: &Value) -> Value {
        
        match objects.as_object() {
            Some(objects_map) => {
                let mut status_map = HashMap::<&str, Value>::new();

                // Per Klipper's API, we shoud use _sub_keys to limit the fields passed
                // in the response. Because we select only the fields we're after in klipper_uds,
                // though, we are simply returning all fields here for simplicity
                for (key, _sub_keys) in objects_map {
                    let val = match key.as_str() {
                        "webhooks" => {
                            json!({
                                "state": self.mock_state.status,
                                "state_message": self.mock_state.status_message
                            })
                        },
                        "gcode_move" => {
                            json!({
                                "gcode_position": [0, 0, self.mock_state.z, 0]
                            })
                        },
                        "curing" => {
                            json!(
                                 self.mock_state.curing
                            )
                        }
                        peripheral => {
                            match self.mock_state.peripheral.get(peripheral) {
                                Some(peripheral_val) => json!(peripheral_val),
                                None => continue,
                            }
                        },
                    };
                    status_map.insert(key, val);
                }
                

                json!({
                    "status": status_map
                })
            },
            None => json!({}),
        }

    }


    pub async fn run(&mut self) -> Result<(), OdysseyError> {
        let mut hardware_sys_time = 0.0;
        let mut read_interval = interval(Duration::from_millis(1_000));
        let mut write_retry_interval = interval(Duration::from_millis(10));
        let mut read_buf: [u8; 1024] = [0; 1024];

        //let _ = self.unix_stream.flush().await;
        loop {
            

            self.unix_stream.readable().await?;
            tracing::info!("hardware mock read loop");

            
            let data_read = match self.unix_stream.try_read(&mut read_buf) {
                Ok(0) => {
                    tracing::info!("Read 0 bytes from Odyssey, indicating connection shutdown. Continuing Anyway.");
                    //break Ok(());
                    read_interval.tick().await;
                    continue;
                }
                Ok(n) => {
                    tracing::debug!("Read {} bytes from Odyssey", n);
                    if let Ok(response_string) = str::from_utf8(&read_buf) {
                        tracing::trace!("Data read: {}", response_string);
                    }
                    &read_buf[0..n]
                }
                Err(e) => match e.kind() {
                    std::io::ErrorKind::WouldBlock => {
                        read_interval.tick().await;
                        continue;
                    }
                    _ => {
                        tracing::error!("Error encountered reading messages from Odyssey:\n{}", e);
                        return Err(e.into());
                    }
                }
            };

            let split_reqs = data_read.split(|&byte| byte == 0x03).filter(|&data_piece| !data_piece.is_empty());

            for req_buf in split_reqs {
                
                if let Ok(response_string) = str::from_utf8(req_buf) {
                    tracing::debug!("Raw request string: {}", response_string);
                }
            
                match serde_json::from_slice::<KlipperRequest>(req_buf) {
                    Ok(req) => {
                        let result = match req.method {
                            KlipperRequestMethod::EmergencyStop =>{
                                json!({
                                })
                            },
                            KlipperRequestMethod::ObjectsList => {
                                json!({

                                })
                            },
                            KlipperRequestMethod::ObjectsQuery => {
                                let objects = req.params.unwrap_or(Value::Null)
                                    .get("objects")
                                    .unwrap_or(&Value::Null).clone();
                                self.object_query_response(&objects)
                            },
                            KlipperRequestMethod::ObjectsSubscribe => {
                                // TODO: add watches here so we only send status updates after getting the subscribe
                                let objects = req.params.unwrap_or(Value::Null)
                                    .get("objects")
                                    .unwrap_or(&Value::Null).clone();
                                self.object_query_response(&objects)

                            },
                            KlipperRequestMethod::GcodeScript => {
                                json!({})
                            },
                            KlipperRequestMethod::GcodeRestart => {
                                Value::Null
                            },
                            KlipperRequestMethod::GcodeFirmwareRestart => {
                                json!({})
                            },
                            KlipperRequestMethod::GcodeSubscribeOutput => {
                                json!({})
                            }
                        };

                        
                        let response = KlipperResponseObject {
                            id: Some(req.id),
                            result: Some(result),
                            source: None,
                            params: None,
                            event_time: None

                        };

                        let _ = self.send_response_retry(response, &mut write_retry_interval, Some(10)).await;
                    },
                    Err(err) => {
                        tracing::debug!(
                            "Encountered Error while parsing Klipper Request: {}",
                            err
                        );
                        if let Ok(response_string) = str::from_utf8(req_buf) {
                            tracing::debug!("Raw response string: {}", response_string);
                        }
                        else {
                            tracing::trace!("Unparsed Data: {:?}", req_buf);
                        }
                    }
                }
            }

            

            tracing::info!("read tick");
            read_interval.tick().await;
            hardware_sys_time+=read_interval.period().as_secs_f64();
        }
    }
}