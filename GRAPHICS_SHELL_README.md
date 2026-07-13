# PySide6 Ephemeral Graphics Shell

A lightweight, modular PySide6-based UI framework for displaying results in ephemeral (temporary, on-demand) QGraphicsView windows. Supports multiple result types with configurable rendering and seamless integration into data pipelines.

## Features

- **Ephemeral Windows**: Create temporary display windows that appear on demand
- **Multiple Result Types**: Text, Charts, Tables, Trees, Grids, and custom renderers
- **Configurable Appearance**: Themes, colors, sizing
- **Auto-Close**: Optional automatic window closure after a timeout
- **Result Management**: Track and manage multiple active shells
- **Data Pipelines**: Chain operations with fluent API
- **CLI Interface**: Command-line tool for shell display
- **Async Support**: Non-blocking rendering operations
- **Custom Renderers**: Extensible renderer system

## Installation

### Requirements
```bash
pip install PySide6
```

### Files
- `graphics_shell.py` - Core graphics shell implementation
- `graphics_shell_utils.py` - Utilities and advanced patterns
- `graphics_shell_cli.py` - Command-line interface

## Quick Start

### Basic Usage

```python
from PySide6.QtWidgets import QApplication
from graphics_shell import ResultType, GraphicsShellManager
import sys

app = QApplication(sys.argv)
manager = GraphicsShellManager()

# Display text result
manager.show_result(
    ResultType.TEXT,
    "Hello from ephemeral shell!",
    title="My Output"
)

sys.exit(app.exec())
```

### Display Different Result Types

```python
# Text
manager.show_result(ResultType.TEXT, "Plain text content", title="Text Output")

# Chart (bar chart from dict or list)
manager.show_result(
    ResultType.CHART,
    {"Python": 45, "Go": 30, "Rust": 25},
    title="Language Stats"
)

# Table (list of lists or list of dicts)
manager.show_result(
    ResultType.TABLE,
    [
        ["ID", "Name", "Score"],
        ["1", "Alice", "95"],
        ["2", "Bob", "87"],
    ],
    title="Results Table"
)

# Tree (hierarchical data)
manager.show_result(
    ResultType.TREE,
    {
        "root": {
            "branch1": {"leaf": "value1"},
            "branch2": ["item1", "item2"]
        }
    },
    title="Data Tree"
)

# Grid (matrix/heatmap visualization)
manager.show_result(
    ResultType.GRID,
    [[1, 2, 3], [4, 5, 6], [7, 8, 9]],
    title="Grid Data"
)
```

### Configuration Options

```python
manager.show_result(
    ResultType.TEXT,
    data,
    title="Custom Shell",
    shell_id="my_shell",           # Identifier for reusing shells
    auto_close_ms=5000,            # Auto-close after 5 seconds
    width=1000,                    # Window width
    height=700,                    # Window height
    theme="dark",                  # "light" or "dark"
    # Or specify colors directly:
    bg_color=QColor(30, 30, 30),   # Background
    fg_color=QColor(255, 255, 255) # Foreground/text
)
```

### Window Management

```python
# Reuse shell by ID (updates content)
shell_id = manager.show_result(ResultType.TEXT, "First", title="Output")
manager.show_result(ResultType.TEXT, "Updated", shell_id=shell_id)

# Close specific shell
manager.close_shell(shell_id)

# Close all shells
manager.close_all()
```

## Advanced Usage

### Data Pipelines

Chain operations with fluent API:

```python
from graphics_shell_utils import ShellPipeline

pipeline = ShellPipeline(manager)
(pipeline
    .load_data({"a": 10, "b": 20, "c": 30})
    .transform(lambda d: {k: v * 2 for k, v in d.items()})
    .filter(lambda v: v > 30)
    .display(ResultType.CHART, "Filtered & Doubled")
)

# Load from file
(ShellPipeline(manager)
    .load_data("data.json")
    .transform(lambda d: sorted(d, key=lambda x: x.get("score", 0), reverse=True))
    .display(ResultType.TABLE, "Top Scores"))
```

### Factory Patterns

```python
from graphics_shell_utils import ShellFactory

# Auto-detect file type
ShellFactory.create_from_file(manager, "data.json")

# Display errors
try:
    risky_operation()
except Exception as e:
    ShellFactory.create_error_shell(manager, e)

# Show statistics
ShellFactory.create_stats_shell(manager, large_dataset, title="Data Stats")
```

### Custom Renderers

```python
from graphics_shell import ResultRenderer, ResultType, RENDERERS
from graphics_shell_utils import CustomResultRenderer

class MyCustomRenderer(CustomResultRenderer):
    @staticmethod
    def render(scene, data, config):
        # Your custom rendering logic here
        # Access scene, data, and config
        pass

# Register custom renderer
RENDERERS[ResultType.CUSTOM] = MyCustomRenderer

# Use it
manager.show_result(ResultType.CUSTOM, data, title="Custom View")
```

### Built-in Custom Renderers

```python
from graphics_shell_utils import register_custom_renderers

register_custom_renderers()

# JSON renderer with syntax highlighting
json_data = {"key": "value", "nested": {"item": 123}}
manager.show_result(ResultType.CUSTOM, json_data, title="JSON View")

# DataFrame renderer
dataframe_data = [
    {"name": "Alice", "age": 30, "city": "NYC"},
    {"name": "Bob", "age": 25, "city": "LA"},
]
manager.show_result(ResultType.CUSTOM, dataframe_data, title="DataFrames")
```

### Async Rendering

```python
import asyncio
from graphics_shell_utils import AsyncShellRenderer

renderer = AsyncShellRenderer()

async def fetch_data():
    await asyncio.sleep(1)  # Simulate async operation
    return {"result": "data"}

# Connect signals
renderer.result_ready.connect(
    lambda config: manager.show_result(
        config.result_type,
        config.data,
        config.title
    )
)
renderer.error_occurred.connect(lambda e: print(f"Error: {e}"))

# Render async
asyncio.run(renderer.render_async(
    fetch_data,
    ResultType.TREE,
    "Async Result"
))
```

## Command-Line Interface

### Basic Commands

```bash
# Text output
python graphics_shell_cli.py --type text --data "Hello World"

# Chart
python graphics_shell_cli.py --type chart --data '{"A": 10, "B": 20}'

# From file
python graphics_shell_cli.py --type table --file data.json

# From stdin
echo '{"key": "value"}' | python graphics_shell_cli.py --type tree --stdin

# Grid
python graphics_shell_cli.py --type grid --data '[[1,2],[3,4]]'
```

### Configuration Options

```bash
# Size and appearance
python graphics_shell_cli.py \
    --type text \
    --data "Content" \
    --width 1000 \
    --height 700 \
    --title "My Window"

# Theme and colors
python graphics_shell_cli.py \
    --type text \
    --data "Dark mode" \
    --theme dark \
    --bg-color "30,30,30" \
    --fg-color "255,255,255"

# Auto-close
python graphics_shell_cli.py \
    --type text \
    --data "Auto-closes in 5 seconds" \
    --auto-close 5000

# JSON output (for scripting)
python graphics_shell_cli.py \
    --type text \
    --data "test" \
    --json-output
```

## API Reference

### ResultType Enum

```python
class ResultType(Enum):
    TEXT = "text"      # Plain text
    CHART = "chart"    # Bar/line charts
    TABLE = "table"    # Tabular data
    TREE = "tree"      # Hierarchical data
    GRID = "grid"      # Matrix/heatmap
    CUSTOM = "custom"  # Custom renderers
```

### ResultConfig Dataclass

```python
@dataclass
class ResultConfig:
    result_type: ResultType    # Type of visualization
    title: str                 # Window title
    data: Any                  # Data to display
    auto_close_ms: int = None  # Auto-close timeout
    width: int = 800           # Window width
    height: int = 600          # Window height
    bg_color: QColor = None    # Background color
    fg_color: QColor = None    # Foreground color
    theme: str = "light"       # Theme: "light" or "dark"
```

### GraphicsShellManager

```python
class GraphicsShellManager:
    def show_result(
        self,
        result_type: Union[str, ResultType],
        data: Any,
        title: str = "Result",
        shell_id: Optional[str] = None,
        auto_close_ms: Optional[int] = None,
        **kwargs
    ) -> str:
        """Display result and return shell ID."""
    
    def close_shell(self, shell_id: str) -> bool:
        """Close specific shell."""
    
    def close_all(self) -> None:
        """Close all shells."""
```

### EphemeralGraphicsShell

```python
class EphemeralGraphicsShell(QMainWindow):
    def __init__(
        self,
        config: ResultConfig,
        parent: Optional[QWidget] = None,
        on_close: Optional[Callable] = None
    ):
        """Create ephemeral shell window."""
    
    def update_result(self, config: ResultConfig) -> None:
        """Update displayed result."""
```

## Examples

### Integration with Data Processing

```python
from graphics_shell import GraphicsShellManager, ResultType
import json

def process_and_display(data_file):
    manager = GraphicsShellManager()
    
    # Load and process
    with open(data_file) as f:
        data = json.load(f)
    
    # Display intermediate result
    manager.show_result(ResultType.TREE, data, title="Raw Data")
    
    # Process
    processed = {
        "total": sum(v for v in data.values() if isinstance(v, (int, float))),
        "count": len(data),
        "items": data
    }
    
    # Display final result
    manager.show_result(ResultType.CHART, 
                       {k: v for k, v in data.items() if isinstance(v, (int, float))},
                       title="Statistics")
```

### Error Visualization

```python
from graphics_shell_utils import ShellFactory

def safe_operation(operation_func):
    manager = GraphicsShellManager()
    try:
        result = operation_func()
        manager.show_result(
            ResultType.TEXT,
            f"Success: {result}",
            title="Operation Result"
        )
    except Exception as e:
        ShellFactory.create_error_shell(manager, e)
```

## Performance Considerations

- **Large Datasets**: Limit rendered items (tables/trees default to 20-30 rows)
- **Auto-Close**: Use `auto_close_ms` to prevent accumulation of windows
- **Async Rendering**: Use `AsyncShellRenderer` for long-running operations
- **Memory**: Close shells when done with `close_shell()` or `close_all()`

## Troubleshooting

### Qt Platform Plugin Not Found
```bash
# Install minimal display support
pip install pyside6[qpa]
```

### Display Issues on Headless Systems
Use Xvfb or similar virtual display:
```bash
xvfb-run -a python graphics_shell_cli.py --type text --data "test"
```

### Custom Renderers Not Showing
Ensure they're registered in RENDERERS dict:
```python
RENDERERS[ResultType.CUSTOM] = MyRenderer
```

## License

MIT - Feel free to use and modify.

## Future Enhancements

- Zooming/panning in views
- Export to image/PDF
- Animation support
- 3D visualization options
- Real-time data streaming
- Network-based remote display
