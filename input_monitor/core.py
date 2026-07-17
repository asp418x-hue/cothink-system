import subprocess
import threading
import time
from typing import Dict, List, Any, Optional, Callable
from collections import deque
from pathlib import Path

from PySide6.QtCore import QObject, Signal

from .models import DeviceType, InputEvent, InputDevice


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
    
    def __init__(self, evdev_script: str = None, 
                 hidraw_script: str = None):
        if evdev_script is None:
            evdev_script = str(Path(__file__).parent / "scripts" / "evdev.sh")
        if hidraw_script is None:
            hidraw_script = str(Path(__file__).parent / "scripts" / "hidraw.sh")
            
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
