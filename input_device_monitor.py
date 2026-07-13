"""
Input Device Monitor - Integration of evdev/hidraw with PySide6 Graphics Shell

Provides real-time monitoring and visualization of Linux input devices (evdev, hidraw)
with interactive QGraphicsView-based UI.
"""

import subprocess
import threading
import time
import json
from typing import Dict, List, Any, Optional, Callable
from dataclasses import dataclass, asdict, field
from datetime import datetime
from enum import Enum
from pathlib import Path
from collections import deque

from PySide6.QtCore import Qt, QThread, Signal, Slot, QTimer, QObject, QMutex
from PySide6.QtWidgets import (
    QMainWindow, QVBoxLayout, QHBoxLayout, QWidget,
    QGraphicsView, QGraphicsScene, QGraphicsItem,
    QGraphicsRectItem, QGraphicsTextItem, QPushButton,
    QComboBox, QLabel, QSpinBox, QCheckBox, QApplication
)
from PySide6.QtGui import QColor, QPen, QBrush, QFont, QKeyEvent

from graphics_shell import ResultType, GraphicsShellManager, EphemeralGraphicsShell, ResultConfig


class DeviceType(Enum):
    """Input device types."""
    KEYBOARD = "keyboard"
    MOUSE = "mouse"
    TOUCHPAD = "touchpad"
    TOUCHSCREEN = "touchscreen"
    JOYSTICK = "joystick"
    HID = "hid"
    UNKNOWN = "unknown"


@dataclass
class InputEvent:
    """Linux input event."""
    timestamp: float
    event_type: int
    event_code: int
    event_value: int
    type_name: str = ""
    code_name: str = ""
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary."""
        return asdict(self)


@dataclass
class InputDevice:
    """Input device information."""
    path: str
    name: str
    device_type: DeviceType
    vendor_id: Optional[str] = None
    product_id: Optional[str] = None
    capabilities: List[str] = field(default_factory=list)
    is_hidraw: bool = False
    last_event: Optional[InputEvent] = None
    event_count: int = 0
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary."""
        data = asdict(self)
        data['device_type'] = self.device_type.value
        data['last_event'] = self.last_event.to_dict() if self.last_event else None
        return data


class BashScript:
    """Wrapper for bash scripts."""
    
    def __init__(self, script_path: str):
        self.script_path = script_path
    
    def execute(self, *args, **kwargs) -> Dict[str, Any]:
        """Execute script and return structured output."""
        try:
            cmd = [self.script_path] + list(args)
            timeout = kwargs.get('timeout', 10)
            
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=timeout
            )
            
            # Parse output
            output = {
                'returncode': result.returncode,
                'stdout': result.stdout,
                'stderr': result.stderr,
                'lines': {}
            }
            
            # Parse key:value format
            for line in result.stdout.strip().split('\n'):
                if ':' in line:
                    key, value = line.split(':', 1)
                    output['lines'][key.strip()] = value.strip()
            
            return output
        except subprocess.TimeoutExpired:
            return {
                'returncode': -1,
                'stderr': f'Timeout after {kwargs.get("timeout", 10)}s',
                'lines': {}
            }
        except Exception as e:
            return {
                'returncode': -1,
                'stderr': str(e),
                'lines': {}
            }


class EventReader(QObject):
    """Background event reader thread."""
    
    events_received = Signal(list)  # List of InputEvent
    device_status_changed = Signal(str, str)  # device, status
    
    def __init__(self, device: InputDevice, script: BashScript):
        super().__init__()
        self.device = device
        self.script = script
        self.running = False
        self.event_buffer = deque(maxlen=100)  # Keep last 100 events
    
    def start_reading(self):
        """Start reading events."""
        self.running = True
        thread = threading.Thread(target=self._read_loop, daemon=True)
        thread.start()
    
    def stop_reading(self):
        """Stop reading events."""
        self.running = False
    
    def _read_loop(self):
        """Main reading loop."""
        while self.running:
            try:
                result = self.script.execute(
                    'read',
                    self.device.path,
                    timeout=2
                )
                
                events = []
                for line in result['stdout'].strip().split('\n'):
                    if line.startswith('event:'):
                        event = self._parse_event(line)
                        if event:
                            events.append(event)
                            self.event_buffer.append(event)
                
                if events:
                    self.events_received.emit(events)
                
                time.sleep(0.1)
            except Exception as e:
                print(f"Error reading events: {e}")
                time.sleep(1)
    
    def _parse_event(self, line: str) -> Optional[InputEvent]:
        """Parse event line."""
        try:
            # Format: event:timestamp=X type=Y code=Z value=W
            parts = line.replace('event:', '').split()
            data = {}
            for part in parts:
                if '=' in part:
                    k, v = part.split('=', 1)
                    data[k] = v
            
            if 'type' in data and 'code' in data and 'value' in data:
                return InputEvent(
                    timestamp=float(data.get('timestamp', time.time())),
                    event_type=int(data['type']),
                    event_code=int(data['code']),
                    event_value=int(data['value']),
                    type_name=self._get_event_type_name(int(data['type'])),
                    code_name=self._get_key_name(int(data['code']))
                )
        except Exception as e:
            print(f"Error parsing event: {e}")
        
        return None
    
    def _get_event_type_name(self, event_type: int) -> str:
        """Get event type name."""
        types = {
            0: "EV_SYN", 1: "EV_KEY", 2: "EV_REL", 3: "EV_ABS",
            4: "EV_MSC", 5: "EV_SW", 17: "EV_LED", 18: "EV_SND"
        }
        return types.get(event_type, f"UNKNOWN({event_type})")
    
    def _get_key_name(self, code: int) -> str:
        """Get key name."""
        keys = {
            1: "ESC", 2: "1", 3: "2", 14: "BACKSPACE", 15: "TAB",
            28: "ENTER", 29: "LCTRL", 42: "LSHIFT", 56: "LALT"
        }
        return keys.get(code, f"KEY_{code}")


class InputDeviceMonitor:
    """Monitor input devices using bash scripts."""
    
    def __init__(self, evdev_script: str = "./evdev.sh", 
                 hidraw_script: str = "./hidraw.sh"):
        self.evdev = BashScript(evdev_script)
        self.hidraw = BashScript(hidraw_script)
        self.devices: Dict[str, InputDevice] = {}
        self.readers: Dict[str, EventReader] = {}
    
    def discover_devices(self) -> List[InputDevice]:
        """Discover all input devices."""
        devices = []
        
        # Discover evdev devices
        result = self.evdev.execute('list')
        for line in result['stdout'].strip().split('\n'):
            if ':' in line:
                path, name = line.split(':', 1)
                devices.append(InputDevice(
                    path=path.strip(),
                    name=name.strip(),
                    device_type=self._infer_device_type(name),
                    is_hidraw=False
                ))
        
        # Discover hidraw devices
        result = self.hidraw.execute('list')
        for line in result['stdout'].strip().split('\n'):
            if ':' in line and '/dev/hidraw' in line:
                path = line.split(':')[0].strip()
                devices.append(InputDevice(
                    path=path,
                    name=f"HID {path}",
                    device_type=DeviceType.HID,
                    is_hidraw=True
                ))
        
        self.devices = {d.path: d for d in devices}
        return devices
    
    def get_device_info(self, device_path: str) -> Optional[Dict[str, Any]]:
        """Get detailed device information."""
        device = self.devices.get(device_path)
        if not device:
            return None
        
        if device.is_hidraw:
            result = self.hidraw.execute('info', device_path)
        else:
            result = self.evdev.execute('info', device_path)
        
        info = device.to_dict()
        info['details'] = result['lines']
        
        return info
    
    def start_monitoring(self, device_path: str, 
                        callback: Optional[Callable] = None) -> EventReader:
        """Start monitoring a device."""
        device = self.devices.get(device_path)
        if not device:
            return None
        
        script = self.hidraw if device.is_hidraw else self.evdev
        reader = EventReader(device, script)
        
        if callback:
            reader.events_received.connect(
                lambda events: callback(device_path, events)
            )
        
        reader.start_reading()
        self.readers[device_path] = reader
        
        return reader
    
    def stop_monitoring(self, device_path: str):
        """Stop monitoring a device."""
        if device_path in self.readers:
            self.readers[device_path].stop_reading()
            del self.readers[device_path]
    
    def _infer_device_type(self, name: str) -> DeviceType:
        """Infer device type from name."""
        name_lower = name.lower()
        if 'keyboard' in name_lower or 'kbd' in name_lower:
            return DeviceType.KEYBOARD
        elif 'mouse' in name_lower:
            return DeviceType.MOUSE
        elif 'touchpad' in name_lower or 'trackpad' in name_lower:
            return DeviceType.TOUCHPAD
        elif 'touchscreen' in name_lower or 'touch' in name_lower:
            return DeviceType.TOUCHSCREEN
        elif 'joystick' in name_lower or 'gamepad' in name_lower:
            return DeviceType.JOYSTICK
        else:
            return DeviceType.UNKNOWN


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
            # Collect stats
            result = subprocess.run(
                ['./evdev.sh', 'stats', device_path, '5'],
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
    import sys
    
    app = QApplication.instance() or QApplication(sys.argv)
    shell_manager = GraphicsShellManager()
    
    control_panel = InputDeviceControlPanel(shell_manager)
    control_panel.show()
    
    sys.exit(app.exec())


if __name__ == "__main__":
    run_input_monitor()
