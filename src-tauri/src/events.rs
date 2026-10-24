//! Event sink abstraction: Tauri emits to the UI; tests collect events in memory.

use serde_json::Value;
use std::sync::Mutex;

pub trait EventSink: Send + Sync {
    fn emit(&self, name: &str, payload: Value);
}

pub struct NoopEvents;
impl EventSink for NoopEvents {
    fn emit(&self, _name: &str, _payload: Value) {}
}

#[derive(Default)]
pub struct CollectEvents {
    pub items: Mutex<Vec<(String, Value)>>,
}
impl EventSink for CollectEvents {
    fn emit(&self, name: &str, payload: Value) {
        self.items.lock().unwrap().push((name.to_string(), payload));
    }
}
impl CollectEvents {
    pub fn names(&self) -> Vec<String> {
        self.items.lock().unwrap().iter().map(|i| i.0.clone()).collect()
    }
}
