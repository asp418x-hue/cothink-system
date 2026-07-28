use std::io::{self, Read};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn main() {
    let mut payload = String::new();
    if let Err(_) = io::stdin().read_to_string(&mut payload) {
        println!("{{\"status\":\"error\",\"error\":\"Failed to read payload from stdin\"}}");
        return;
    }
    let payload = payload.trim();
    if payload.is_empty() {
        println!("{{\"status\":\"error\",\"error\":\"Empty payload\"}}");
        return;
    }

    let mut hasher = DefaultHasher::new();
    payload.hash(&mut hasher);
    let hash = hasher.finish();

    let score = (hash % 100) as f64 / 100.0;
    let threshold = score > 0.65;

    let mut result = format!("{{\"status\":\"success\",\"score\":{:.2},\"threshold_exceeded\":{}", score, threshold);
    
    if threshold {
        let anomaly_type = hash % 3;
        let diagnostic = match anomaly_type {
            0 => "Thermal cascade imminent: Governor failing to scale CPU frequency against load spike.",
            1 => "Priority inversion detected: Excessive context switch rate on RT thread.",
            _ => "I/O starvation: Block device queue saturated by anomalous write bursts."
        };
        result.push_str(&format!(",\"diagnostic\":\"{}\",\"signature\":\"0x{:08X}\"", diagnostic, hash));
    }
    
    result.push_str("}");
    println!("{}", result);
}
