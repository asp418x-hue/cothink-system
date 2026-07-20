use std::sync::Arc;
use std::time::SystemTime;

/// The Immutable Reference Frame.
/// Once initialized, the payload is read-only for all subscriber agents.
#[derive(Debug, Clone)]
pub struct SensorFrame {
    pub timestamp: SystemTime,
    pub frame_id: u64,
    pub raw_data: Arc<Vec<u8>>, // Arc ensures immutable shared access
    pub sensor_metadata: FrameMetadata,
}

#[derive(Debug, Clone, Copy)]
pub struct FrameMetadata {
    pub resolution: (u32, u32),
    pub sensor_id: u32,
    pub frequency_hz: f64,
}

impl SensorFrame {
    pub fn new(id: u64, data: Vec<u8>, meta: FrameMetadata) -> Self {
        Self {
            timestamp: SystemTime::now(),
            frame_id: id,
            raw_data: Arc::new(data),
            sensor_metadata: meta,
        }
    }
}
