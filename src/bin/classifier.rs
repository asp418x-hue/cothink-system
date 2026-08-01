use std::sync::Arc;
use std::time::SystemTime;

/// Anomaly score output by classification agents
#[derive(Debug, Clone)]
pub struct AnomalyScore {
    pub score: f64,
    pub threshold_crossed: bool,
}

/// The Immutable Reference Frame.
/// Once initialized, the payload is read-only for all subscriber agents.
#[derive(Debug, Clone)]
pub struct SensorFrame {
    pub timestamp: SystemTime,
    pub frame_id: u64,
    pub raw_data: Arc<Vec<u8>>, // Arc ensures immutable shared access
    pub anomaly_score: AnomalyScore,
}

/// Agent operates on read-only snapshots
fn agent_classifier(frame: Arc<SensorFrame>) {
    // Agent reads frame, performs analysis, never mutates.
    println!("[Rust Agent] Reading frame ID {}...", frame.frame_id);
    println!("[Rust Agent] Payload size: {} bytes", frame.raw_data.len());

    // Simulate classification processing time
    std::thread::sleep(std::time::Duration::from_millis(150));

    println!(
        "[Rust Agent] Analysis anomaly score: {:.2} (Threshold crossed: {})",
        frame.anomaly_score.score, frame.anomaly_score.threshold_crossed
    );
}

/// Load simulated sensor data
fn load_sensor_data(id: u64) -> SensorFrame {
    // Mocking raw data vector
    let mock_bytes = vec![0x1A, 0x2B, 0x3C, 0x4D, 0x5E];

    // Simulate anomaly score calculation based on frame ID
    let score = (id as f64 * 0.07).min(1.0);
    let threshold_crossed = score > 0.5;

    SensorFrame {
        timestamp: SystemTime::now(),
        frame_id: id,
        raw_data: Arc::new(mock_bytes),
        anomaly_score: AnomalyScore {
            score,
            threshold_crossed,
        },
    }
}

fn main() {
    // Read command line arguments to get frame ID
    let args: Vec<String> = std::env::args().collect();
    let frame_id: u64 = if args.len() > 1 {
        args[1].parse().unwrap_or(42)
    } else {
        42
    };

    let raw_data = Arc::new(load_sensor_data(frame_id));

    // Multiple agents referencing the same memory block
    // No copying, no mutation, no decoherence.
    let handle = std::thread::spawn(move || {
        agent_classifier(raw_data.clone());
    });

    handle.join().unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anomaly_score_math() {
        let frame = load_sensor_data(1);
        assert_eq!(frame.frame_id, 1);
        assert!((frame.anomaly_score.score - 0.07).abs() < 1e-6);
        assert!(!frame.anomaly_score.threshold_crossed);

        let high_frame = load_sensor_data(8);
        assert!(high_frame.anomaly_score.threshold_crossed);
    }

    #[test]
    fn test_shared_raw_data() {
        let frame = Arc::new(load_sensor_data(1));
        let data_clone = frame.raw_data.clone();
        assert_eq!(data_clone.len(), 5);
        assert_eq!(data_clone[0], 0x1A);
    }
}
