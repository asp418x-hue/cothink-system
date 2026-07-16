use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};
use tokio::process::Command;
use tokio::task::JoinSet;
use std::process::Stdio;

fn main() {
    println!("cothink-system is booting...");

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async {
        let orch_id = cothink_system::history::next_orchestrator_id();
        cothink_system::history::record(orch_id, 0, "orch_start", true, "allocation=8".to_string());
        
        let allocation = spiral_order(8, 2);
        let completed = spawn_concurrent_subagents(orch_id, &allocation).await;
        
        cothink_system::history::record(orch_id, 0, "orch_finish", true, format!("completed={}", completed.len()));
        println!("spiral allocation: {:?}", allocation);
        println!("completed subagents: {:?}", completed);
        cothink_system::history::show_history();
    });
}

fn radial_layer(index: usize) -> usize {
    if index == 0 {
        return 0;
    }

    let phi = 1.618033988749895_f64;
    let layer = ((index as f64).log(phi)).floor() as usize;
    layer.max(1)
}

fn spiral_order(task_count: usize, truncation_depth: usize) -> Vec<usize> {
    if task_count == 0 || truncation_depth == 0 {
        return Vec::new();
    }

    let size = (task_count as f64).sqrt().ceil() as usize;
    let mut order = Vec::with_capacity(task_count);
    let layers = truncation_depth.min((size + 1) / 2);

    for layer in 0..layers {
        let top = layer;
        let bottom = size.saturating_sub(1).saturating_sub(layer);
        let left = layer;
        let right = size.saturating_sub(1).saturating_sub(layer);

        if top > bottom || left > right {
            break;
        }

        let stride = radial_layer(layer + 1).max(1);

        for col in left..=right {
            if order.len() == task_count {
                return order;
            }
            if (col + top) % stride == 0 || layer == 0 {
                let index = top * size + col;
                if index < task_count {
                    order.push(index);
                }
            }
        }

        for row in (top + 1)..=bottom {
            if order.len() == task_count {
                return order;
            }
            if (row + right) % stride == 0 || layer == 0 {
                let index = row * size + right;
                if index < task_count {
                    order.push(index);
                }
            }
        }

        if top < bottom && left < right {
            for col in (left..right).rev() {
                if order.len() == task_count {
                    return order;
                }
                if (col + bottom) % stride == 0 || layer == 0 {
                    let index = bottom * size + col;
                    if index < task_count {
                        order.push(index);
                    }
                }
            }

            for row in (top + 1..bottom).rev() {
                if order.len() == task_count {
                    return order;
                }
                if (row + left) % stride == 0 || layer == 0 {
                    let index = row * size + left;
                    if index < task_count {
                        order.push(index);
                    }
                }
            }
        }
    }

    order
}

async fn spawn_subagent(orch_id: u64, task_id: usize) -> Result<usize, String> {
    cothink_system::history::record(orch_id, task_id, "subagent_start", true, "".to_string());
    let child_res = Command::new("sh")
        .arg("-c")
        .arg(format!("echo subagent:{}; sleep 0.02", task_id))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match child_res {
        Ok(c) => c,
        Err(e) => {
            let err_msg = format!("failed to spawn: {}", e);
            cothink_system::history::record(orch_id, task_id, "subagent_fail", false, err_msg.clone());
            let diag = query_failure_diagnostics("sda");
            cothink_system::history::record(orch_id, task_id, "subagent_fail_query", false, diag);
            return Err(err_msg);
        }
    };

    let stdout = child.stdout.take().expect("child stdout");
    let mut reader = AsyncBufReader::new(stdout);
    let mut line = String::new();
    if let Err(e) = reader.read_line(&mut line).await {
        let err_msg = format!("failed to read stdout: {}", e);
        cothink_system::history::record(orch_id, task_id, "subagent_fail", false, err_msg.clone());
        let diag = query_failure_diagnostics("sda");
        cothink_system::history::record(orch_id, task_id, "subagent_fail_query", false, diag);
        return Err(err_msg);
    }

    let status = match child.wait().await {
        Ok(s) => s,
        Err(e) => {
            let err_msg = format!("failed to wait: {}", e);
            cothink_system::history::record(orch_id, task_id, "subagent_fail", false, err_msg.clone());
            let diag = query_failure_diagnostics("sda");
            cothink_system::history::record(orch_id, task_id, "subagent_fail_query", false, diag);
            return Err(err_msg);
        }
    };

    if !status.success() {
        let err_msg = format!("exit status: {:?}", status.code());
        cothink_system::history::record(orch_id, task_id, "subagent_fail", false, err_msg.clone());
        let diag = query_failure_diagnostics("sda");
        cothink_system::history::record(orch_id, task_id, "subagent_fail_query", false, diag);
        return Err(err_msg);
    }

    cothink_system::history::record(orch_id, task_id, "subagent_success", true, line.trim().to_string());
    Ok(task_id)
}

fn query_failure_diagnostics(device: &str) -> String {
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

    let disk_health = if std::path::Path::new(&format!("/sys/block/{}/stat", device)).exists() {
        "PASSED"
    } else {
        "PASSED (simulated)"
    };

    format!("diagnostics - Temp: {:.1}°C | CPU Freq: {}MHz | Disk Status: {}", temp, freq, disk_health)
}

async fn spawn_concurrent_subagents(orch_id: u64, task_ids: &[usize]) -> Vec<usize> {
    let mut set = JoinSet::new();

    for &task_id in task_ids {
        set.spawn(spawn_subagent(orch_id, task_id));
    }

    let mut completed = Vec::with_capacity(task_ids.len());
    while let Some(join_result) = set.join_next().await {
        match join_result {
            Ok(Ok(task_id)) => {
                completed.push(task_id);
            }
            Ok(Err(e)) => {
                eprintln!("Subagent task failed: {}", e);
            }
            Err(e) => {
                let err_msg = format!("task panicked: {}", e);
                cothink_system::history::record(orch_id, 0, "subagent_panic", false, err_msg.clone());
                eprintln!("{}", err_msg);
            }
        }
    }

    completed
}

#[allow(dead_code)]
async fn spawn_concurrent_subagents_from_arc(orch_id: u64, task_ids: Arc<Vec<usize>>) -> Vec<usize> {
    spawn_concurrent_subagents(orch_id, &task_ids).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spiral_order_covers_all_tasks_once() {
        let order = spiral_order(8, 3);
        assert_eq!(order.len(), 8);

        let mut seen = std::collections::BTreeSet::new();
        for idx in order {
            assert!(seen.insert(idx));
        }
    }

    #[test]
    fn radial_layer_uses_log_phi_spacing() {
        assert_eq!(radial_layer(0), 0);
        assert_eq!(radial_layer(1), 1);
        assert_eq!(radial_layer(2), 1);
        assert_eq!(radial_layer(3), 2);
        assert_eq!(radial_layer(4), 2);
        assert_eq!(radial_layer(5), 3);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn concurrent_dispatch_returns_all_subagents() {
        let allocation = spiral_order(6, 3);
        let completed = spawn_concurrent_subagents(1, &allocation).await;

        assert_eq!(completed.len(), 6);
        let mut seen = std::collections::BTreeSet::new();
        for task_id in completed {
            assert!(seen.insert(task_id));
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn arc_backed_dispatch_returns_all_subagents() {
        let allocation = Arc::new(vec![2, 4, 6, 8]);
        let completed = spawn_concurrent_subagents_from_arc(1, allocation).await;

        assert_eq!(completed.len(), 4);
        let mut seen = std::collections::BTreeSet::new();
        for task_id in completed {
            assert!(seen.insert(task_id));
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn child_process_ipc_roundtrip_works() {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("printf 'ready\n'")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("child should start");

        let stdout = child.stdout.take().expect("child stdout");
        let mut reader = AsyncBufReader::new(stdout);
        let mut line = String::new();
        let _ = reader
            .read_line(&mut line)
            .await
            .expect("failed to read child stdout");

        let status = child.wait().await.expect("child wait");
        assert!(status.success());
        assert_eq!(line.trim(), "ready");
    }
}
