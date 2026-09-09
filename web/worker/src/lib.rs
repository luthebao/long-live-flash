use llflash_core::FloatDuration;
use llflash_core::Player;
use llflash_core::worker::{
    MessageChannelId, WebWorkerBootstrap, WebWorkerCommand, WorkerWireValue,
    build_web_worker_player,
};
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Avm2WorkerInstance {
    player: Arc<Mutex<Player>>,
    previous_timestamp: Option<f64>,
}

#[wasm_bindgen]
impl Avm2WorkerInstance {
    #[wasm_bindgen(constructor)]
    pub fn new(bootstrap: JsValue) -> Result<Avm2WorkerInstance, JsValue> {
        let bootstrap: WebWorkerBootstrap = serde_wasm_bindgen::from_value(bootstrap)
            .map_err(|error| JsValue::from_str(&format!("Invalid AVM2 worker bootstrap: {error}")))?;
        let player = build_web_worker_player(bootstrap).map_err(|error| JsValue::from_str(&error))?;
        Ok(Self {
            player,
            previous_timestamp: None,
        })
    }

    pub fn tick(&mut self, timestamp: f64) {
        let dt = self
            .previous_timestamp
            .map_or(0.0, |previous| (timestamp - previous).max(0.0));
        self.previous_timestamp = Some(timestamp);
        self.player
            .lock()
            .unwrap()
            .tick(FloatDuration::from_millis(dt));
    }

    #[wasm_bindgen(js_name = "timeTilNextFrameMs")]
    pub fn time_til_next_frame_ms(&self) -> f64 {
        self.player
            .lock()
            .unwrap()
            .time_til_next_frame()
            .as_secs_f64()
            * 1000.0
    }

    #[wasm_bindgen(js_name = "takeCommands")]
    pub fn take_commands(&self) -> Result<JsValue, JsValue> {
        let commands: Vec<WebWorkerCommand> = self
            .player
            .lock()
            .unwrap()
            .take_web_worker_commands();
        serde_wasm_bindgen::to_value(&commands)
            .map_err(|error| JsValue::from_str(&format!("Unable to serialize Worker commands: {error}")))
    }

    #[wasm_bindgen(js_name = "injectMessage")]
    pub fn inject_message(
        &self,
        channel_id: MessageChannelId,
        value: JsValue,
    ) -> Result<(), JsValue> {
        let value: WorkerWireValue = serde_wasm_bindgen::from_value(value)
            .map_err(|error| JsValue::from_str(&format!("Invalid Worker message: {error}")))?;
        self.player
            .lock()
            .unwrap()
            .inject_web_worker_message(channel_id, value)
            .map_err(|error| JsValue::from_str(&format!("Unable to inject Worker message: {error:?}")))
    }

    #[wasm_bindgen(js_name = "closeChannel")]
    pub fn close_channel(&self, channel_id: MessageChannelId) -> bool {
        self.player
            .lock()
            .unwrap()
            .inject_web_channel_close(channel_id)
    }
}
