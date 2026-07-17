import subprocess
import sys
from collections import deque
from typing import Dict, List, Any

from PySide6.QtCore import QTimer
from PySide6.QtWidgets import (
    QMainWindow, QVBoxLayout, QHBoxLayout, QWidget,
    QGraphicsView, QGraphicsScene, QGraphicsItem,
    QGraphicsRectItem, QGraphicsTextItem, QPushButton,
    QComboBox, QLabel, QApplication
)
from PySide6.QtGui import QColor, QPen, QBrush, QFont

from graphics_shell import ResultType, GraphicsShellManager

from .models import InputDevice, InputEvent
from .core import InputDeviceMonitor


class InputDeviceVisualizerWidget(QGraphicsView):
    """QGraphicsView widget for input device visualization."""
    
    def __init__(self, manager: GraphicsShellManager, parent=None):
        super().__init__(parent)
        self.manager = manager
        self.monitor = InputDeviceMonitor()
        self.selected_device = None
        self.event_history = deque(maxlen=50)
        
        # Setup scene
        self.scene = QGraphicsScene()
        self.setScene(self.scene)
        self.setRenderHint(self.RenderHint.Antialiasing)
        
        # Setup timer for updates
        self.update_timer = QTimer()
        self.update_timer.timeout.connect(self._update_display)
        self.update_timer.start(500)
        
        # Discover devices
        self._refresh_devices()
    
    def _refresh_devices(self):
        """Refresh device list."""
        devices = self.monitor.discover_devices()
        self._draw_device_list(devices)
    
    def _draw_device_list(self, devices: List[InputDevice]):
        """Draw device list in scene."""
        self.scene.clear()
        
        y = 20
        title = QGraphicsTextItem("Input Devices")
        title_font = QFont("Arial", 14)
        title_font.setBold(True)
        title.setFont(title_font)
        self.scene.addItem(title)
        title.setPos(20, 5)
        
        y = 40
        for device in devices:
            self._draw_device_item(device, y)
            y += 60
    
    def _draw_device_item(self, device: InputDevice, y: int):
        """Draw a device item."""
        x = 20
        
        # Background
        bg = QGraphicsRectItem(x, y, 500, 50)
        bg.setBrush(QBrush(QColor(240, 240, 240)))
        bg.setPen(QPen(QColor(100, 100, 100)))
        self.scene.addItem(bg)
        
        # Device name
        name_text = QGraphicsTextItem(f"{device.name} ({device.device_type.value})")
        name_text.setFont(QFont("Arial", 11))
        self.scene.addItem(name_text)
        name_text.setPos(x + 10, y + 5)
        
        # Device path
        path_text = QGraphicsTextItem(device.path)
        path_text.setFont(QFont("Courier", 9))
        path_text.setDefaultTextColor(QColor(100, 100, 100))
        self.scene.addItem(path_text)
        path_text.setPos(x + 10, y + 25)
        
        # Status indicator
        status_color = QColor(0, 200, 0) if device.is_hidraw else QColor(0, 100, 200)
        status = QGraphicsRectItem(x + 480, y + 10, 20, 30)
        status.setBrush(QBrush(status_color))
        self.scene.addItem(status)
    
    def _update_display(self):
        """Update display."""
        if self.selected_device:
            info = self.monitor.get_device_info(self.selected_device)
            if info and info.get('last_event'):
                self._draw_event_info(info)
    
    def _draw_event_info(self, info: Dict[str, Any]):
        """Draw event information."""
        pass  # Update scene with event info
    
    def select_device(self, device_path: str):
        """Select a device for monitoring."""
        self.selected_device = device_path
        
        # Start monitoring
        self.monitor.start_monitoring(
            device_path,
            self._on_events_received
        )
    
    def _on_events_received(self, device_path: str, events: List[InputEvent]):
        """Handle events received."""
        for event in events:
            self.event_history.append(event)
            self.manager.show_result(
                ResultType.TEXT,
                f"Event: {event.type_name} {event.code_name} = {event.event_value}",
                title=f"Input: {device_path}",
                shell_id=f"input_{device_path}",
                width=600,
                height=200
            )


class InputDeviceControlPanel(QMainWindow):
    """Control panel for input device monitoring."""
    
    def __init__(self, manager: GraphicsShellManager, parent=None):
        super().__init__(parent)
        self.manager = manager
        self.monitor = InputDeviceMonitor()
        
        self.setWindowTitle("Input Device Monitor")
        self.setGeometry(100, 100, 1000, 700)
        
        # Central widget
        central = QWidget()
        self.setCentralWidget(central)
        
        layout = QVBoxLayout(central)
        
        # Controls
        controls_layout = QHBoxLayout()
        
        # Device selection
        device_label = QLabel("Device:")
        self.device_combo = QComboBox()
        self.device_combo.currentIndexChanged.connect(self._on_device_selected)
        
        # Buttons
        refresh_btn = QPushButton("Refresh")
        refresh_btn.clicked.connect(self._refresh_devices)
        
        monitor_btn = QPushButton("Start Monitoring")
        monitor_btn.clicked.connect(self._start_monitoring)
        
        stats_btn = QPushButton("Show Statistics")
        stats_btn.clicked.connect(self._show_statistics)
        
        controls_layout.addWidget(device_label)
        controls_layout.addWidget(self.device_combo)
        controls_layout.addWidget(refresh_btn)
        controls_layout.addWidget(monitor_btn)
        controls_layout.addWidget(stats_btn)
        controls_layout.addStretch()
        
        layout.addLayout(controls_layout)
        
        # Graphics view
        self.viewer = InputDeviceVisualizerWidget(manager)
        layout.addWidget(self.viewer)
        
        # Refresh devices
        self._refresh_devices()
    
    def _refresh_devices(self):
        """Refresh device list."""
        devices = self.monitor.discover_devices()
        
        self.device_combo.clear()
        for device in devices:
            self.device_combo.addItem(
                f"{device.name} ({device.device_type.value})",
                device.path
            )
        
        self.viewer._refresh_devices()
    
    def _on_device_selected(self, index: int):
        """Handle device selection."""
        if index >= 0:
            device_path = self.device_combo.itemData(index)
            self.viewer.select_device(device_path)
    
    def _start_monitoring(self):
        """Start monitoring selected device."""
        device_path = self.device_combo.currentData()
        if device_path:
            self.monitor.start_monitoring(device_path)
            
            # Show info in graphics shell
            info = self.monitor.get_device_info(device_path)
            if info:
                self.manager.show_result(
                    ResultType.TREE,
                    info,
                    title=f"Monitoring: {info['name']}",
                    width=800,
                    height=600
                )
    
    def _show_statistics(self):
        """Show device statistics."""
        device_path = self.device_combo.currentData()
        if device_path:
            from pathlib import Path
            evdev_script = str(Path(__file__).parent / "scripts" / "evdev.sh")
            
            # Collect stats
            result = subprocess.run(
                [evdev_script, 'stats', device_path, '5'],
                capture_output=True,
                text=True,
                timeout=10
            )
            
            stats = {}
            for line in result.stdout.strip().split('\n'):
                if ':' in line:
                    k, v = line.split(':', 1)
                    stats[k.strip()] = v.strip()
            
            self.manager.show_result(
                ResultType.TREE,
                stats,
                title="Device Statistics",
                width=700,
                height=500
            )


def run_input_monitor():
    """Run input device monitor application."""
    app = QApplication.instance() or QApplication(sys.argv)
    shell_manager = GraphicsShellManager()
    
    control_panel = InputDeviceControlPanel(shell_manager)
    control_panel.show()
    
    sys.exit(app.exec())
