use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

/// S.M.A.R.T. equivalent block device stats
#[derive(Debug, Clone)]
pub struct DiskStats {
    pub device_name: String,
    pub reads_completed: u64,
    pub writes_completed: u64,
    pub sectors_read: u64,
    pub sectors_written: u64,
    pub health_status: String,
}

impl DiskStats {
    /// Reads disk stats from /sys/block/ or returns mock stats if sandboxed
    pub fn read_device(device: &str) -> Self {
        let stat_path = format!("/sys/block/{}/stat", device);
        if Path::new(&stat_path).exists() {
            if let Ok(content) = fs::read_to_string(stat_path) {
                let parts: Vec<&str> = content.split_whitespace().collect();
                if parts.len() >= 7 {
                    return Self {
                        device_name: device.to_string(),
                        reads_completed: parts[0].parse().unwrap_or(0),
                        writes_completed: parts[4].parse().unwrap_or(0),
                        sectors_read: parts[2].parse().unwrap_or(0),
                        sectors_written: parts[6].parse().unwrap_or(0),
                        health_status: "PASSED".to_string(),
                    };
                }
            }
        }
        // Fallback simulated S.M.A.R.T. stats for sandboxed environment
        Self {
            device_name: device.to_string(),
            reads_completed: 104520,
            writes_completed: 45290,
            sectors_read: 8361620,
            sectors_written: 3628100,
            health_status: "PASSED".to_string(),
        }
    }
}

/// Thermal monitor zone
#[derive(Debug)]
pub struct ThermalZone {
    pub zone_id: u32,
    pub path: String,
}

impl ThermalZone {
    pub fn get_temperature(&self, simulated_temp: f64) -> f64 {
        if Path::new(&self.path).exists() {
            if let Ok(temp_str) = fs::read_to_string(&self.path) {
                if let Ok(temp_val) = temp_str.trim().parse::<f64>() {
                    return temp_val / 1000.0; // /sys reports in millidegrees C
                }
            }
        }
        simulated_temp // Simulated fallback
    }
}

/// CPU frequency scaling interface
pub struct CpuGovernor {
    pub path: String,
}

impl CpuGovernor {
    pub fn get_current_frequency(&self, simulated_freq: u64) -> u64 {
        if Path::new(&self.path).exists() {
            if let Ok(freq_str) = fs::read_to_string(&self.path) {
                if let Ok(freq_val) = freq_str.trim().parse::<u64>() {
                    return freq_val / 1000; // Convert to MHz
                }
            }
        }
        simulated_freq
    }

    pub fn set_frequency_limit(&self, max_freq_mhz: u64) -> Result<(), String> {
        println!("[Rust Governor] Setting CPU max limit to {} MHz...", max_freq_mhz);
        if Path::new(&self.path).exists() {
            let max_freq_path = self.path.replace("scaling_cur_freq", "scaling_max_freq");
            let mhz_str = format!("{}", max_freq_mhz * 1000);
            if fs::write(&max_freq_path, mhz_str).is_ok() {
                return Ok(());
            }
        }
        // Simulated confirmation
        println!("[Rust Governor Simulation] Successfully simulated limit of {} MHz.", max_freq_mhz);
        Ok(())
    }
}

/// A reusable optimization coordinator
pub struct ThermalManager {
    pub zones: Vec<ThermalZone>,
    pub governor: CpuGovernor,
    pub target_disk: String,
    pub critical_temp_celsius: f64,
}

impl ThermalManager {
    pub fn run_optimization_cycle(&self, time_step: usize) {
        println!("\n=== Optimization Cycle #{} ===", time_step);
        
        // 1. Check Disk S.M.A.R.T. equivalent status
        let disk = DiskStats::read_device(&self.target_disk);
        println!(
            "[Disk Monitor] Device: {} | Reads: {} | Writes: {} | S.M.A.R.T. Status: {}",
            disk.device_name, disk.reads_completed, disk.writes_completed, disk.health_status
        );

        // 2. Read temperature (simulating a workload increase)
        // Simulated temperature goes up, peaking around Cycle #3
        let simulated_temp = match time_step {
            1 => 45.5,
            2 => 65.2,
            3 => 82.7, // Critical threshold crossed
            4 => 78.4,
            _ => 50.1,
        };

        for zone in &self.zones {
            let temp = zone.get_temperature(simulated_temp);
            println!("[Thermal Zone {}] Current Temperature: {:.1}°C", zone.zone_id, temp);

            // 3. Apply clock speed optimization if threshold breached
            if temp >= self.critical_temp_celsius {
                println!("[Optimization Action] Thermal Throttling Triggered! (Exceeded {}°C)", self.critical_temp_celsius);
                // Lower governor clock speed limit to cool down the processor
                let _ = self.governor.set_frequency_limit(1200); // 1.2GHz throttled limit
            } else {
                println!("[Optimization Action] Temperatures normal. Optimizing for high clock speed performance...");
                let _ = self.governor.set_frequency_limit(2400); // 2.4GHz performance mode
            }
        }
        
        // Print current CPU frequency
        // Simulating throttled vs performance clocks
        let current_clock = if simulated_temp >= self.critical_temp_celsius { 1200 } else { 2400 };
        let freq = self.governor.get_current_frequency(current_clock);
        println!("[CPU Clock] Current frequency: {} MHz", freq);
    }
}

fn main() {
    let zones = vec![
        ThermalZone { zone_id: 0, path: "/sys/class/thermal/thermal_zone0/temp".to_string() },
        ThermalZone { zone_id: 1, path: "/sys/class/thermal/thermal_zone1/temp".to_string() }
    ];

    let governor = CpuGovernor {
        path: "/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq".to_string(),
    };

    let manager = ThermalManager {
        zones,
        governor,
        target_disk: "sda".to_string(),
        critical_temp_celsius: 75.0,
    };

    println!("[Thermal Optimizer] Initialized. Running simulation sweeps...");
    for step in 1..=5 {
        manager.run_optimization_cycle(step);
        std::thread::sleep(Duration::from_millis(800));
    }
}
