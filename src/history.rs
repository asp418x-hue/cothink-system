use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::SystemTime;

const HISTORY_CAPACITY: usize = 4096;

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub timestamp: SystemTime,
    pub orchestrator_id: u64,
    pub subagent_id: usize,
    pub event: String,
    pub success: bool,
    pub detail: String,
}

pub struct HistoryTracker {
    buffer: Vec<Mutex<Option<HistoryEntry>>>,
    cursor: AtomicUsize,
    active_workers: AtomicUsize,
}

impl HistoryTracker {
    pub fn new() -> Self {
        let mut buffer = Vec::with_capacity(HISTORY_CAPACITY);
        for _ in 0..HISTORY_CAPACITY {
            buffer.push(Mutex::new(None));
        }
        Self {
            buffer,
            cursor: AtomicUsize::new(0),
            active_workers: AtomicUsize::new(0),
        }
    }

    pub fn record_event(
        &self,
        orch_id: u64,
        subagent_id: usize,
        event: &str,
        success: bool,
        detail: String,
    ) {
        if event == "subagent_start" {
            self.active_workers.fetch_add(1, Ordering::SeqCst);
        } else if event == "subagent_success" || event == "subagent_fail" || event == "subagent_cancel" {
            self.active_workers.fetch_sub(1, Ordering::SeqCst);
        }

        let seq = self.cursor.fetch_add(1, Ordering::Relaxed);
        let idx = seq % HISTORY_CAPACITY;
        
        let mut slot = self.buffer[idx].lock().unwrap();
        *slot = Some(HistoryEntry {
            timestamp: SystemTime::now(),
            orchestrator_id: orch_id,
            subagent_id,
            event: event.to_string(),
            success,
            detail,
        });
    }

    pub fn get_history(&self) -> Vec<HistoryEntry> {
        let cursor = self.cursor.load(Ordering::Relaxed);
        let start = if cursor > HISTORY_CAPACITY {
            cursor - HISTORY_CAPACITY
        } else {
            0
        };

        let mut results = Vec::new();
        for i in start..cursor {
            let idx = i % HISTORY_CAPACITY;
            if let Some(entry) = &*self.buffer[idx].lock().unwrap() {
                results.push(entry.clone());
            }
        }
        results
    }

    pub fn clear(&self) {
        self.cursor.store(0, Ordering::Relaxed);
        self.active_workers.store(0, Ordering::Relaxed);
        for slot in &self.buffer {
            let mut val = slot.lock().unwrap();
            *val = None;
        }
    }
}

static TRACKER: std::sync::OnceLock<HistoryTracker> = std::sync::OnceLock::new();
static NEXT_ORCH_ID: AtomicU64 = AtomicU64::new(1);

pub fn get_tracker() -> &'static HistoryTracker {
    TRACKER.get_or_init(HistoryTracker::new)
}

pub fn record(orch_id: u64, subagent_id: usize, event: &str, success: bool, detail: String) {
    get_tracker().record_event(orch_id, subagent_id, event, success, detail);
}

pub fn next_orchestrator_id() -> u64 {
    NEXT_ORCH_ID.fetch_add(1, Ordering::SeqCst)
}

#[derive(Debug, Clone)]
pub struct FOV {
    pub active_workers: usize,
    pub temp_celsius: f64,
    pub cpu_freq_mhz: u64,
    pub recent_success: usize,
    pub recent_failure: usize,
}

pub fn get_fov(orch_id: u64, _device: &str) -> FOV {
    let tracker = get_tracker();
    let active = tracker.active_workers.load(Ordering::Relaxed);

    let temp = if let Ok(t_str) = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp") {
        t_str.trim().parse::<f64>().unwrap_or(0.0) / 1000.0
    } else {
        0.0
    };

    let freq = if let Ok(f_str) = std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq") {
        f_str.trim().parse::<u64>().unwrap_or(0) / 1000
    } else {
        0
    };

    let history = tracker.get_history();
    let mut successes = 0;
    let mut failures = 0;
    for entry in history {
        if entry.orchestrator_id == orch_id {
            if entry.event == "subagent_success" {
                successes += 1;
            } else if entry.event == "subagent_fail" {
                failures += 1;
            }
        }
    }

    FOV {
        active_workers: active,
        temp_celsius: temp,
        cpu_freq_mhz: freq,
        recent_success: successes,
        recent_failure: failures,
    }
}

pub fn show_history() {
    let history = get_tracker().get_history();
    println!("\n--- Execution History (Rust) ---");
    for entry in history {
        let success_str = if entry.success { "SUCCESS" } else { "FAIL" };
        let time_str = match entry.timestamp.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(d) => format!("{}.{:03}s", d.as_secs() % 86400, d.subsec_millis()),
            Err(_) => "0s".to_string(),
        };
        println!(
            "  [{}] Orch: {} | Agent: {} | Event: {} | Status: {} | Detail: {}",
            time_str, entry.orchestrator_id, entry.subagent_id, entry.event, success_str, entry.detail
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_concurrent_history_recording() {
        let tracker = get_tracker();
        tracker.clear();

        let num_threads = 20;
        let writes_per_thread = 50;
        let mut handles = vec![];

        for t_idx in 0..num_threads {
            handles.push(thread::spawn(move || {
                for i in 0..writes_per_thread {
                    record(t_idx as u64, i, "test_rust_event", true, format!("payload_{}", i));
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let history = tracker.get_history();
        let expected_count = (num_threads * writes_per_thread).min(HISTORY_CAPACITY);
        assert_eq!(history.len(), expected_count);

        for entry in history {
            assert_eq!(entry.event, "test_rust_event");
            assert!(entry.detail.starts_with("payload_"));
        }
    }
}
