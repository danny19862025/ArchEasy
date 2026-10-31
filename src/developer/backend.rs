//! Developer manifest and embedded script adapter; no GTK dependency.
use crate::gaming::backend::{Session, start_script};
use serde_json::Value;
use std::sync::mpsc::Receiver;
const MANIFEST: &str = include_str!("../../data/developer-tools.json");
const SCRIPT: &str = include_str!("../../scripts/developer.py");
const RUNTIME: &str = include_str!("../../scripts/gaming.py");

pub fn catalog() -> Value {
    serde_json::from_str(MANIFEST).expect("bundled developer manifest")
}
fn source() -> String {
    format!(
        "import types, sys, json\n_runtime = types.ModuleType('archeasy_setup')\nexec({}, _runtime.__dict__)\nsys.modules['archeasy_setup'] = _runtime\nMANIFEST = json.loads({})\n{}",
        serde_json::to_string(RUNTIME).expect("runtime string"),
        serde_json::to_string(MANIFEST).expect("manifest string"),
        SCRIPT
    )
}
pub fn start(mode: &str, request: Value) -> Result<(Session, Receiver<Value>), String> {
    start_script(&source(), mode, &[request.to_string()])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_developer_manifest_and_protocol() {
        let manifest = catalog();
        assert_eq!(
            manifest["presets"]["AI Developer"]
                .as_array()
                .unwrap()
                .len(),
            8
        );
        let (_session, events) = start("catalog", serde_json::json!({})).unwrap();
        assert_eq!(
            events.recv().unwrap()["catalog"].as_array().unwrap().len(),
            manifest["tools"].as_array().unwrap().len()
        );
        assert_eq!(events.recv().unwrap()["done"], true);
        assert_eq!(events.recv().unwrap()["exited"], true);
    }
}
