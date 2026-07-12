use std::io::{BufRead, BufReader};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};
use tokio::process::Command;
use tokio::task::JoinSet;
use std::process::Stdio;

fn main() {
    println!("cothink-system is booting...");

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async {
        let allocation = spiral_order(8, 2);
        let completed = spawn_concurrent_subagents(&allocation).await;
        println!("spiral allocation: {:?}", allocation);
        println!("completed subagents: {:?}", completed);
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

async fn spawn_subagent(task_id: usize) -> usize {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(format!("echo subagent:{}; sleep 0.02", task_id))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn child process");

    let stdout = child.stdout.take().expect("child stdout");
    let mut reader = AsyncBufReader::new(stdout);
    let mut line = String::new();
    let _ = reader
        .read_line(&mut line)
        .await
        .expect("failed to read child stdout");

    let status = child.wait().await.expect("child wait");
    assert!(status.success(), "subagent child failed");
    task_id
}

async fn spawn_concurrent_subagents(task_ids: &[usize]) -> Vec<usize> {
    let mut set = JoinSet::new();

    for &task_id in task_ids {
        set.spawn(spawn_subagent(task_id));
    }

    let mut completed = Vec::with_capacity(task_ids.len());
    while let Some(join_result) = set.join_next().await {
        completed.push(join_result.expect("subagent task panicked"));
    }

    completed
}

async fn spawn_concurrent_subagents_from_arc(task_ids: Arc<Vec<usize>>) -> Vec<usize> {
    spawn_concurrent_subagents(&task_ids).await
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
        let completed = spawn_concurrent_subagents(&allocation).await;

        assert_eq!(completed.len(), 6);
        let mut seen = std::collections::BTreeSet::new();
        for task_id in completed {
            assert!(seen.insert(task_id));
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn arc_backed_dispatch_returns_all_subagents() {
        let allocation = Arc::new(vec![2, 4, 6, 8]);
        let completed = spawn_concurrent_subagents_from_arc(allocation).await;

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
