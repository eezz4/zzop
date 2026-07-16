mod options;
mod overlays;
mod severity;
mod shape;
mod topology;

fn analyze_request(v: &serde_json::Value) -> &serde_json::Map<String, serde_json::Value> {
    v.as_object().expect("Analyze request must be an object")
}
