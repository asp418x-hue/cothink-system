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
        let (diagnostic, solution) = match anomaly_type {
            0 => (
                "Thermal cascade imminent: Governor failing to scale CPU frequency against load spike.",
                "1. Audit cpufreq governor settings (e.g. switch from 'performance' to 'schedutil').\\n2. Profile the instruction payload for infinite loops or unyielded tight polling.\\n3. Check for blocked thermal daemon (thermald)."
            ),
            1 => (
                "Priority inversion detected: Excessive context switch rate on RT thread.",
                "1. Inspect futex contention in the payload execution path.\\n2. Demote the RT (Real-Time) scheduling policy (SCHED_FIFO/SCHED_RR) of the faulting thread to SCHED_OTHER.\\n3. Use priority inheritance mutexes (PI-mutex) if lock sharing is mandatory."
            ),
            _ => (
                "I/O starvation: Block device queue saturated by anomalous write bursts.",
                "1. Implement application-level write batching or buffering.\\n2. Tune 'dirty_ratio' and 'dirty_background_ratio' in sysctl.\\n3. Profile fsync() calls in the payload execution path."
            )
        };
        result.push_str(&format!(",\"diagnostic\":\"{}\",\"solution\":\"{}\",\"signature\":\"0x{:08X}\"", diagnostic, solution, hash));
    }
    
    result.push_str("}");
    println!("{}", result);
}
