#!/usr/bin/env python3
"""
Practical Examples: Graphics Shell Integration with Data Pipelines

This module demonstrates real-world usage patterns for the ephemeral graphics shell,
including integration with data processing, monitoring, and system analysis.
"""

import json
import time
import random
from typing import Dict, List, Any
from dataclasses import dataclass, asdict
from pathlib import Path

from PySide6.QtWidgets import QApplication
from PySide6.QtGui import QColor

from graphics_shell import ResultType, GraphicsShellManager
from graphics_shell_utils import ShellPipeline, ShellFactory


# Example 1: System Monitoring Dashboard
class SystemMonitor:
    """Monitor system metrics and display them."""
    
    def __init__(self, manager: GraphicsShellManager):
        self.manager = manager
        self.metrics_history: List[Dict[str, Any]] = []
    
    def collect_metrics(self) -> Dict[str, Any]:
        """Collect system metrics (simulated)."""
        metrics = {
            "timestamp": time.time(),
            "cpu_usage": random.randint(10, 90),
            "memory_usage": random.randint(20, 80),
            "disk_usage": random.randint(30, 70),
            "network_in": random.randint(1, 100),
            "network_out": random.randint(1, 100),
        }
        self.metrics_history.append(metrics)
        return metrics
    
    def display_current(self) -> None:
        """Display current metrics."""
        if not self.metrics_history:
            self.collect_metrics()
        
        metrics = self.metrics_history[-1]
        clean_metrics = {k: v for k, v in metrics.items() if k != "timestamp"}
        
        self.manager.show_result(
            ResultType.CHART,
            clean_metrics,
            title="System Metrics (Current)",
            shell_id="system_metrics",
            width=700,
            height=500
        )
    
    def display_history(self) -> None:
        """Display metrics history as table."""
        if not self.metrics_history:
            for _ in range(5):
                self.collect_metrics()
                time.sleep(0.1)
        
        self.manager.show_result(
            ResultType.TABLE,
            self.metrics_history[-10:],
            title="Metrics History",
            width=900,
            height=600
        )
    
    def display_alerts(self, thresholds: Dict[str, int]) -> None:
        """Display alerts for metrics exceeding thresholds."""
        if not self.metrics_history:
            self.collect_metrics()
        
        current = self.metrics_history[-1]
        alerts = {k: v for k, v in current.items() 
                 if k in thresholds and v > thresholds[k]}
        
        if alerts:
            self.manager.show_result(
                ResultType.TREE,
                {"alerts": alerts, "timestamp": current["timestamp"]},
                title="System Alerts",
                bg_color=QColor(255, 240, 240),
                fg_color=QColor(200, 0, 0)
            )
        else:
            self.manager.show_result(
                ResultType.TEXT,
                "✓ All systems nominal",
                title="System Status",
                bg_color=QColor(240, 255, 240),
                fg_color=QColor(0, 128, 0)
            )


# Example 2: Data Analysis Pipeline
class DataAnalyzer:
    """Analyze datasets and visualize results."""
    
    def __init__(self, manager: GraphicsShellManager):
        self.manager = manager
    
    def analyze_dataset(self, data: List[Dict[str, Any]]) -> None:
        """Analyze and display dataset statistics."""
        if not data:
            return
        
        # Calculate statistics
        stats = {
            "record_count": len(data),
            "fields": list(data[0].keys()) if data else [],
            "sample": data[0] if data else {},
        }
        
        self.manager.show_result(
            ResultType.TREE,
            stats,
            title="Dataset Analysis"
        )
    
    def compare_datasets(self, data1: List[Dict], data2: List[Dict],
                        key: str) -> None:
        """Compare two datasets."""
        comparison = {
            "dataset1": {
                "count": len(data1),
                "sample": data1[0] if data1 else None,
            },
            "dataset2": {
                "count": len(data2),
                "sample": data2[0] if data2 else None,
            },
            "difference_count": len(data1) - len(data2),
        }
        
        self.manager.show_result(
            ResultType.TREE,
            comparison,
            title="Dataset Comparison"
        )
    
    def show_distribution(self, data: List[Any], title: str = "Distribution") -> None:
        """Show distribution of values."""
        # Create histogram-like data
        bins = {}
        for value in data:
            if isinstance(value, (int, float)):
                bin_key = int(value / 10) * 10
                bins[bin_key] = bins.get(bin_key, 0) + 1
        
        self.manager.show_result(
            ResultType.CHART,
            bins,
            title=title
        )


# Example 3: Experiment Results Display
class ExperimentTracker:
    """Track and display experimental results."""
    
    def __init__(self, manager: GraphicsShellManager):
        self.manager = manager
        self.experiments: Dict[str, Dict[str, Any]] = {}
    
    def record_experiment(self, name: str, parameters: Dict[str, Any],
                         results: Dict[str, Any], timestamp: float = None) -> None:
        """Record an experiment."""
        self.experiments[name] = {
            "parameters": parameters,
            "results": results,
            "timestamp": timestamp or time.time(),
        }
    
    def display_experiment(self, name: str) -> None:
        """Display single experiment details."""
        if name not in self.experiments:
            self.manager.show_result(
                ResultType.TEXT,
                f"Experiment '{name}' not found",
                title="Error"
            )
            return
        
        exp = self.experiments[name]
        self.manager.show_result(
            ResultType.TREE,
            exp,
            title=f"Experiment: {name}"
        )
    
    def display_results_comparison(self) -> None:
        """Compare all experiment results."""
        comparison = {}
        for name, exp in self.experiments.items():
            comparison[name] = exp.get("results", {})
        
        self.manager.show_result(
            ResultType.TREE,
            comparison,
            title="Experiments Comparison",
            width=900,
            height=700
        )
    
    def display_metrics_over_time(self, metric_key: str) -> None:
        """Plot metric over time."""
        metrics = {}
        for name, exp in sorted(self.experiments.items(), 
                               key=lambda x: x[1].get("timestamp", 0)):
            if metric_key in exp.get("results", {}):
                metrics[name] = exp["results"][metric_key]
        
        self.manager.show_result(
            ResultType.CHART,
            metrics,
            title=f"'{metric_key}' Over Time"
        )


# Example 4: Configuration and Status Viewer
class ConfigurationViewer:
    """Display and manage configurations."""
    
    def __init__(self, manager: GraphicsShellManager):
        self.manager = manager
        self.configs: Dict[str, Dict[str, Any]] = {}
    
    def load_config(self, name: str, filepath: str) -> None:
        """Load configuration from file."""
        try:
            with open(filepath) as f:
                if filepath.endswith('.json'):
                    self.configs[name] = json.load(f)
                else:
                    self.configs[name] = {"raw": f.read()}
        except Exception as e:
            self.manager.show_result(
                ResultType.TEXT,
                f"Error loading config: {e}",
                title="Config Load Error"
            )
    
    def display_config(self, name: str) -> None:
        """Display configuration."""
        if name not in self.configs:
            self.manager.show_result(
                ResultType.TEXT,
                f"Configuration '{name}' not found",
                title="Error"
            )
            return
        
        self.manager.show_result(
            ResultType.TREE,
            self.configs[name],
            title=f"Configuration: {name}"
        )
    
    def display_all_configs(self) -> None:
        """Display all loaded configurations."""
        self.manager.show_result(
            ResultType.TREE,
            self.configs,
            title="All Configurations"
        )


# Example 5: Log Viewer
class LogViewer:
    """Display and analyze logs."""
    
    def __init__(self, manager: GraphicsShellManager):
        self.manager = manager
        self.logs: Dict[str, List[str]] = {}
    
    def load_log(self, name: str, filepath: str) -> None:
        """Load log file."""
        try:
            with open(filepath) as f:
                self.logs[name] = f.readlines()
        except Exception as e:
            self.manager.show_result(
                ResultType.TEXT,
                f"Error loading log: {e}",
                title="Log Load Error"
            )
    
    def display_log_tail(self, name: str, lines: int = 50) -> None:
        """Display last N lines of log."""
        if name not in self.logs:
            self.manager.show_result(
                ResultType.TEXT,
                f"Log '{name}' not found",
                title="Error"
            )
            return
        
        log_lines = self.logs[name][-lines:]
        content = "".join(log_lines)
        
        self.manager.show_result(
            ResultType.TEXT,
            content,
            title=f"Log: {name} (last {lines} lines)",
            width=1000,
            height=700
        )
    
    def display_log_summary(self, name: str) -> None:
        """Display log summary with statistics."""
        if name not in self.logs:
            self.manager.show_result(
                ResultType.TEXT,
                f"Log '{name}' not found",
                title="Error"
            )
            return
        
        logs = self.logs[name]
        
        # Count log levels (simple heuristic)
        summary = {
            "total_lines": len(logs),
            "errors": sum(1 for line in logs if "error" in line.lower()),
            "warnings": sum(1 for line in logs if "warn" in line.lower()),
            "info": sum(1 for line in logs if "info" in line.lower()),
        }
        
        self.manager.show_result(
            ResultType.CHART,
            {k: v for k, v in summary.items() if v > 0},
            title=f"Log Summary: {name}"
        )


# Example 6: Thermal/Performance Data Display (relevant to your cothink-system)
class ThermalMonitor:
    """Monitor thermal and performance data."""
    
    def __init__(self, manager: GraphicsShellManager):
        self.manager = manager
        self.thermal_data: List[Dict[str, float]] = []
    
    def record_thermal(self, cpu_temp: float, gpu_temp: float,
                      clock_freq: float, power_usage: float) -> None:
        """Record thermal/performance metrics."""
        self.thermal_data.append({
            "timestamp": time.time(),
            "cpu_temp": cpu_temp,
            "gpu_temp": gpu_temp,
            "clock_freq": clock_freq,
            "power_usage": power_usage,
        })
    
    def display_current_thermal(self) -> None:
        """Display current thermal state."""
        if not self.thermal_data:
            return
        
        current = self.thermal_data[-1]
        temps = {
            "CPU": current["cpu_temp"],
            "GPU": current["gpu_temp"],
        }
        
        self.manager.show_result(
            ResultType.CHART,
            temps,
            title="Thermal Status",
            shell_id="thermal_current"
        )
    
    def display_thermal_history(self, last_n: int = 20) -> None:
        """Display thermal history."""
        self.manager.show_result(
            ResultType.TABLE,
            self.thermal_data[-last_n:],
            title="Thermal History",
            width=900,
            height=500
        )
    
    def display_thermal_warning(self, cpu_threshold: float = 85.0,
                               gpu_threshold: float = 80.0) -> None:
        """Display thermal warnings."""
        if not self.thermal_data:
            return
        
        current = self.thermal_data[-1]
        warnings = {}
        
        if current["cpu_temp"] > cpu_threshold:
            warnings["CPU_OVERHEAT"] = current["cpu_temp"]
        if current["gpu_temp"] > gpu_threshold:
            warnings["GPU_OVERHEAT"] = current["gpu_temp"]
        
        if warnings:
            self.manager.show_result(
                ResultType.TREE,
                {"warnings": warnings, "thresholds": 
                 {"cpu": cpu_threshold, "gpu": gpu_threshold}},
                title="⚠ Thermal Warnings",
                bg_color=QColor(255, 200, 0),
                fg_color=QColor(0, 0, 0)
            )


def run_all_examples():
    """Run all example demonstrations."""
    from PySide6.QtCore import QTimer
    
    app = QApplication.instance() or QApplication([])
    manager = GraphicsShellManager()
    
    # Example 1: System Monitor
    monitor = SystemMonitor(manager)
    monitor.display_current()
    
    # Example 2: Data Analyzer
    analyzer = DataAnalyzer(manager)
    sample_data = [
        {"id": i, "value": random.randint(10, 100), "category": chr(65 + (i % 3))}
        for i in range(10)
    ]
    analyzer.analyze_dataset(sample_data)
    analyzer.show_distribution([random.randint(0, 100) for _ in range(100)])
    
    # Example 3: Experiment Tracker
    tracker = ExperimentTracker(manager)
    tracker.record_experiment(
        "exp_1",
        {"learning_rate": 0.01, "batch_size": 32},
        {"accuracy": 0.95, "loss": 0.05}
    )
    tracker.record_experiment(
        "exp_2",
        {"learning_rate": 0.001, "batch_size": 64},
        {"accuracy": 0.97, "loss": 0.03}
    )
    tracker.display_results_comparison()
    
    # Example 4: Thermal Monitor
    thermal = ThermalMonitor(manager)
    thermal.record_thermal(45.0, 65.0, 2400, 150)
    thermal.record_thermal(50.0, 70.0, 2600, 160)
    thermal.display_current_thermal()
    
    print("Examples displayed. Close windows to exit.")
    app.exec()


if __name__ == "__main__":
    run_all_examples()
