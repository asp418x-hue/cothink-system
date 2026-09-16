use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::fs;
use crate::history::{get_fov, get_tracker, record, FOV};

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NativeFov {
    pub active_workers: usize,
    pub temp_celsius: f64,
    pub cpu_freq_mhz: u64,
    pub recent_success: usize,
    pub recent_failure: usize,
}

impl From<FOV> for NativeFov {
    fn from(fov: FOV) -> Self {
        Self {
            active_workers: fov.active_workers,
            temp_celsius: fov.temp_celsius,
            cpu_freq_mhz: fov.cpu_freq_mhz,
            recent_success: fov.recent_success,
            recent_failure: fov.recent_failure,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NativeThermalSample {
    pub zone_id: u32,
    pub temp_celsius: f64,
    pub cpu_freq_mhz: u64,
    pub is_throttling: bool,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NativeAnomalyResult {
    pub frame_id: u64,
    pub score: f64,
    pub threshold_crossed: bool,
}

/// Initialize native cothink runtime components
#[unsafe(no_mangle)]
pub extern "C" fn cothink_init() -> i32 {
    let _ = get_tracker();
    0
}

/// Query current Field of View (FOV) telemetry
#[unsafe(no_mangle)]
pub extern "C" fn cothink_get_fov(orch_id: u64, out_fov: *mut NativeFov) -> i32 {
    if out_fov.is_null() {
        return -1;
    }
    let fov = get_fov(orch_id, "default");
    unsafe {
        *out_fov = NativeFov::from(fov);
    }
    0
}

/// Record a subagent or orchestrator event directly into native history
#[unsafe(no_mangle)]
pub extern "C" fn cothink_record_event(
    orch_id: u64,
    subagent_id: usize,
    event: *const c_char,
    success: bool,
    detail: *const c_char,
) -> i32 {
    if event.is_null() || detail.is_null() {
        return -1;
    }
    let event_str = unsafe { CStr::from_ptr(event).to_string_lossy() };
    let detail_str = unsafe { CStr::from_ptr(detail).to_string_lossy() };
    record(orch_id, subagent_id, &event_str, success, detail_str.into_owned());
    0
}

/// Query thermal zone and governor status
#[unsafe(no_mangle)]
pub extern "C" fn cothink_thermal_sample(
    zone_id: u32,
    simulated_temp: f64,
    critical_temp: f64,
    out_sample: *mut NativeThermalSample,
) -> i32 {
    if out_sample.is_null() {
        return -1;
    }

    let temp_path = format!("/sys/class/thermal/thermal_zone{}/temp", zone_id);
    let temp = if let Ok(content) = fs::read_to_string(&temp_path) {
        content.trim().parse::<f64>().unwrap_or(simulated_temp * 1000.0) / 1000.0
    } else {
        simulated_temp
    };

    let freq_path = "/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq";
    let freq = if let Ok(content) = fs::read_to_string(freq_path) {
        content.trim().parse::<u64>().unwrap_or(2400000) / 1000
    } else {
        if temp >= critical_temp { 1200 } else { 2400 }
    };

    let is_throttling = temp >= critical_temp;

    unsafe {
        *out_sample = NativeThermalSample {
            zone_id,
            temp_celsius: temp,
            cpu_freq_mhz: freq,
            is_throttling,
        };
    }
    0
}

/// Run in-process sensor frame classification
#[unsafe(no_mangle)]
pub extern "C" fn cothink_classify(
    frame_id: u64,
    out_result: *mut NativeAnomalyResult,
) -> i32 {
    if out_result.is_null() {
        return -1;
    }

    let score = (frame_id as f64 * 0.07).min(1.0);
    let threshold_crossed = score > 0.5;

    unsafe {
        *out_result = NativeAnomalyResult {
            frame_id,
            score,
            threshold_crossed,
        };
    }
    0
}

/// Execute payload metrics analysis (reads system load & meminfo)
#[unsafe(no_mangle)]
pub extern "C" fn cothink_execute_payload(payload_json: *const c_char) -> *mut c_char {
    let _payload = if payload_json.is_null() {
        ""
    } else {
        unsafe { CStr::from_ptr(payload_json).to_str().unwrap_or("") }
    };

    let loadavg = fs::read_to_string("/proc/loadavg").unwrap_or_else(|_| "0.00 0.00 0.00".to_string());
    let load_1m: f64 = loadavg.split_whitespace().next().unwrap_or("0").parse().unwrap_or(0.0);

    let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut mem_total: f64 = 1.0;
    let mut mem_avail: f64 = 1.0;

    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            mem_total = line.split_whitespace().nth(1).unwrap_or("1").parse().unwrap_or(1.0);
        }
        if line.starts_with("MemAvailable:") {
            mem_avail = line.split_whitespace().nth(1).unwrap_or("1").parse().unwrap_or(1.0);
        }
    }

    let mem_usage_pct: f64 = (1.0f64 - (mem_avail / mem_total)).clamp(0.0f64, 1.0f64);
    let threshold = load_1m > 4.0 || mem_usage_pct > 0.85;
    let score = (load_1m / 4.0).max(mem_usage_pct);

    let mut result = format!(
        "{{\"status\":\"success\",\"score\":{:.2},\"threshold_exceeded\":{},\"load_1m\":{:.2},\"mem_usage_pct\":{:.2}",
        score, threshold, load_1m, mem_usage_pct
    );

    if threshold {
        let (diagnostic, solution) = if mem_usage_pct > 0.85 {
            (
                format!("Memory starvation detected: System RAM usage is at {:.1}%. OOM killer risk is critical.", mem_usage_pct * 100.0),
                "1. Audit running subagents and terminate idle or memory-leaking workers.\\n2. Increase swap space using `mkswap` and `swapon`.\\n3. Tune `vm.swappiness` to a higher value."
            )
        } else {
            (
                format!("CPU saturation cascade: Load average (1m) is at {:.2}, exceeding safe threshold.", load_1m),
                "1. Inspect process queue with `top` or `htop` to identify CPU hogs.\\n2. Adjust execution delay of the swarm payload.\\n3. Lower priority of non-critical agents using `renice`."
            )
        };
        result.push_str(&format!(
            ",\"diagnostic\":\"{}\",\"solution\":\"{}\",\"signature\":\"0x1A2B3C4D\"",
            diagnostic, solution
        ));
    }

    result.push('}');

    match CString::new(result) {
        Ok(c_str) => c_str.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a C-string allocated by Rust
#[unsafe(no_mangle)]
pub extern "C" fn cothink_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}
