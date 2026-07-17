from typing import Dict, List, Any, Optional
from dataclasses import dataclass, asdict, field
from enum import Enum

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
