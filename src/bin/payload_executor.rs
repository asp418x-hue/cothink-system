use std::io::{self, Read};
use std::fs;

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

    // Read actual system metrics
    let loadavg = fs::read_to_string("/proc/loadavg").unwrap_or_else(|_| "0.00 0.00 0.00".to_string());
    let load_1m: f64 = loadavg.split_whitespace().next().unwrap_or("0").parse().unwrap_or(0.0);

    let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut mem_total = 1.0;
    let mut mem_avail = 1.0;
    
    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            mem_total = line.split_whitespace().nth(1).unwrap_or("1").parse().unwrap_or(1.0);
        }
        if line.starts_with("MemAvailable:") {
            mem_avail = line.split_whitespace().nth(1).unwrap_or("1").parse().unwrap_or(1.0);
        }
    }
    
    let mem_usage_pct = 1.0 - (mem_avail / mem_total);
    
    // We determine anomalies based on ACTUAL system conditions
    let threshold = load_1m > 4.0 || mem_usage_pct > 0.85;
    let score = (load_1m / 4.0).max(mem_usage_pct);

    let mut result = format!("{{\"status\":\"success\",\"score\":{:.2},\"threshold_exceeded\":{}", score, threshold);
    
    if threshold {
        let (diagnostic, solution) = if mem_usage_pct > 0.85 {
            (
                format!("Memory starvation detected: System RAM usage is at {:.1}%. OOM killer risk is critical.", mem_usage_pct * 100.0),
                "1. Audit running subagents and terminate idle or memory-leaking workers.\\n2. Increase swap space using `mkswap` and `swapon`.\\n3. Tune `vm.swappiness` to a higher value to reclaim page cache."
            )
        } else {
            (
                format!("CPU saturation cascade: Load average (1m) is at {:.2}, exceeding safe threshold.", load_1m),
                "1. Inspect process queue with `top` or `htop` to identify CPU hogs.\\n2. Adjust execution delay of the swarm payload.\\n3. Lower priority of non-critical agents using `renice`."
            )
        };
        result.push_str(&format!(",\"diagnostic\":\"{}\",\"solution\":\"{}\",\"signature\":\"0x{:08X}\"", diagnostic, solution, 0x1A2B3C4D));
    }
    
    result.push_str("}");
    println!("{}", result);
}
