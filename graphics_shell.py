"""
PySide6 QGraphicsView Ephemeral Shell

An ephemeral (temporary, on-demand) UI shell for displaying various result types
in a QGraphicsView. Supports different layouts and result renderers based on
arguments passed at instantiation.
"""

import sys
import json
from typing import Any, Callable, Dict, List, Optional, Union
from dataclasses import dataclass
from enum import Enum

from PySide6.QtWidgets import (
    QApplication, QGraphicsView, QGraphicsScene, QGraphicsItem,
    QGraphicsTextItem, QGraphicsRectItem, QGraphicsPathItem,
    QWidget, QVBoxLayout, QMainWindow, QMessageBox
)
from PySide6.QtCore import Qt, QTimer, QPointF, QSize, Signal, QObject, Slot
from PySide6.QtGui import QColor, QPen, QBrush, QFont, QPainterPath


class ResultType(Enum):
    """Supported result visualization types."""
    TEXT = "text"
    CHART = "chart"
    TABLE = "table"
    TREE = "tree"
    GRID = "grid"
    CUSTOM = "custom"


@dataclass
class ResultConfig:
    """Configuration for result rendering."""
    result_type: ResultType
    title: str
    data: Any
    auto_close_ms: Optional[int] = None
    width: int = 800
    height: int = 600
    bg_color: QColor = None
    fg_color: QColor = None
    theme: str = "light"
    
    def __post_init__(self):
        if self.bg_color is None:
            self.bg_color = QColor(240, 240, 240) if self.theme == "light" else QColor(30, 30, 30)
        if self.fg_color is None:
            self.fg_color = QColor(0, 0, 0) if self.theme == "light" else QColor(255, 255, 255)


class GraphicsShellSignals(QObject):
    """Signals for the graphics shell."""
    closed = Signal()
    result_rendered = Signal(str)


class ResultRenderer:
    """Abstract base for result type renderers."""
    
    @staticmethod
    def render(scene: QGraphicsScene, data: Any, config: ResultConfig) -> None:
        raise NotImplementedError


class TextResultRenderer(ResultRenderer):
    """Renders text-based results."""
    
    @staticmethod
    def render(scene: QGraphicsScene, data: Any, config: ResultConfig) -> None:
        text_content = str(data) if not isinstance(data, str) else data
        text_item = QGraphicsTextItem(text_content)
        
        font = QFont("Courier", 10)
        text_item.setFont(font)
        text_item.setDefaultTextColor(config.fg_color)
        
        scene.addItem(text_item)
        text_item.setPos(20, 20)


class ChartResultRenderer(ResultRenderer):
    """Renders simple bar/line chart results."""
    
    @staticmethod
    def render(scene: QGraphicsScene, data: Any, config: ResultConfig) -> None:
        if isinstance(data, dict):
            items = list(data.items())
        elif isinstance(data, list):
            items = [(str(i), v) for i, v in enumerate(data)]
        else:
            items = [("value", data)]
        
        bar_width = 40
        bar_spacing = 20
        max_height = config.height - 100
        
        if items:
            max_value = max(float(v) if isinstance(v, (int, float)) else 1 for _, v in items)
            max_value = max(max_value, 1)
        
        x_pos = 50
        for label, value in items:
            value_num = float(value) if isinstance(value, (int, float)) else 1
            bar_height = (value_num / max_value) * max_height
            
            # Draw bar
            bar = QGraphicsRectItem(x_pos, config.height - 80 - bar_height, bar_width, bar_height)
            bar.setPen(QPen(config.fg_color))
            bar.setBrush(QBrush(QColor(100, 150, 200)))
            scene.addItem(bar)
            
            # Draw label
            label_item = QGraphicsTextItem(str(label)[:10])
            label_font = QFont("Arial", 8)
            label_item.setFont(label_font)
            label_item.setDefaultTextColor(config.fg_color)
            scene.addItem(label_item)
            label_item.setPos(x_pos, config.height - 70)
            
            x_pos += bar_width + bar_spacing


class TableResultRenderer(ResultRenderer):
    """Renders tabular data."""
    
    @staticmethod
    def render(scene: QGraphicsScene, data: Any, config: ResultConfig) -> None:
        rows = []
        if isinstance(data, list):
            rows = data
        elif isinstance(data, dict):
            rows = [[k, v] for k, v in data.items()]
        else:
            rows = [[str(data)]]
        
        row_height = 25
        col_width = config.width // 2
        x, y = 20, 20
        
        for row_idx, row in enumerate(rows[:20]):  # Limit to 20 rows
            cells = row if isinstance(row, (list, tuple)) else [row]
            for col_idx, cell in enumerate(cells[:4]):  # Limit to 4 columns
                # Cell background
                cell_bg = QGraphicsRectItem(x + col_idx * col_width, y + row_idx * row_height,
                                           col_width, row_height)
                cell_bg.setPen(QPen(config.fg_color, 1))
                cell_bg.setBrush(QBrush(QColor(250, 250, 250) if row_idx % 2 == 0 
                                       else QColor(240, 240, 240)))
                scene.addItem(cell_bg)
                
                # Cell text
                cell_text = QGraphicsTextItem(str(cell)[:30])
                cell_text.setFont(QFont("Arial", 9))
                cell_text.setDefaultTextColor(config.fg_color)
                scene.addItem(cell_text)
                cell_text.setPos(x + col_idx * col_width + 5, y + row_idx * row_height + 3)


class TreeResultRenderer(ResultRenderer):
    """Renders hierarchical tree data."""
    
    @staticmethod
    def render(scene: QGraphicsScene, data: Any, config: ResultConfig) -> None:
        def draw_tree(node: Any, x: float, y: float, indent: float = 30) -> float:
            if isinstance(node, dict):
                for key, value in node.items():
                    text = QGraphicsTextItem(f"▶ {key}")
                    text.setFont(QFont("Arial", 10))
                    text.setDefaultTextColor(config.fg_color)
                    scene.addItem(text)
                    text.setPos(x, y)
                    y += 25
                    
                    if isinstance(value, (dict, list)):
                        y = draw_tree(value, x + indent, y, indent)
                    else:
                        text_val = QGraphicsTextItem(f"  {value}")
                        text_val.setFont(QFont("Courier", 9))
                        text_val.setDefaultTextColor(QColor(100, 100, 100))
                        scene.addItem(text_val)
                        text_val.setPos(x + indent, y)
                        y += 20
            elif isinstance(node, list):
                for i, item in enumerate(node[:20]):
                    text = QGraphicsTextItem(f"[{i}]")
                    text.setFont(QFont("Arial", 10))
                    text.setDefaultTextColor(config.fg_color)
                    scene.addItem(text)
                    text.setPos(x, y)
                    y += 25
                    
                    if isinstance(item, (dict, list)):
                        y = draw_tree(item, x + indent, y, indent)
                    else:
                        text_val = QGraphicsTextItem(f"  {item}")
                        text_val.setFont(QFont("Courier", 9))
                        text_val.setDefaultTextColor(QColor(100, 100, 100))
                        scene.addItem(text_val)
                        text_val.setPos(x + indent, y)
                        y += 20
            
            return y
        
        draw_tree(data, 30, 20)


class GridResultRenderer(ResultRenderer):
    """Renders grid-based data visualization."""
    
    @staticmethod
    def render(scene: QGraphicsScene, data: Any, config: ResultConfig) -> None:
        grid_size = 20
        x, y = 50, 50
        
        if isinstance(data, (list, tuple)):
            for i, row in enumerate(data[:30]):
                for j, cell in enumerate((row if isinstance(row, (list, tuple)) else [row])[:30]):
                    cell_rect = QGraphicsRectItem(x + j * grid_size, y + i * grid_size,
                                                 grid_size, grid_size)
                    value = float(cell) if isinstance(cell, (int, float)) else 0
                    intensity = min(255, int(abs(value) * 50))
                    cell_rect.setBrush(QBrush(QColor(intensity, intensity, intensity)))
                    cell_rect.setPen(QPen(config.fg_color, 0.5))
                    scene.addItem(cell_rect)


RENDERERS: Dict[ResultType, type] = {
    ResultType.TEXT: TextResultRenderer,
    ResultType.CHART: ChartResultRenderer,
    ResultType.TABLE: TableResultRenderer,
    ResultType.TREE: TreeResultRenderer,
    ResultType.GRID: GridResultRenderer,
}


class EphemeralGraphicsShell(QMainWindow):
    """
    Ephemeral graphics shell window using QGraphicsView.
    
    Appears on demand, displays results based on configuration, and can
    auto-close or remain until manually closed.
    """
    
    def __init__(self, config: ResultConfig, parent: Optional[QWidget] = None,
                 on_close: Optional[Callable] = None):
        super().__init__(parent)
        self.config = config
        self.on_close_callback = on_close
        self.signals = GraphicsShellSignals()
        
        # Setup window
        self.setWindowTitle(f"Shell: {config.title}")
        self.setGeometry(100, 100, config.width, config.height)
        
        # Create graphics view and scene
        self.scene = QGraphicsScene()
        self.scene.setBackgroundBrush(QBrush(config.bg_color))
        
        self.view = QGraphicsView(self.scene)
        self.view.setRenderHint(self.view.RenderHint.Antialiasing)
        self.setCentralWidget(self.view)
        
        # Render results
        self._render_results()
        
        # Setup auto-close if specified
        if config.auto_close_ms:
            QTimer.singleShot(config.auto_close_ms, self.close)
    
    def _render_results(self) -> None:
        """Render results based on configuration."""
        # Add title
        title_item = QGraphicsTextItem(self.config.title)
        title_font = QFont("Arial", 14)
        title_font.setBold(True)
        title_item.setFont(title_font)
        title_item.setDefaultTextColor(self.config.fg_color)
        self.scene.addItem(title_item)
        title_item.setPos(20, 5)
        
        # Add separator line
        line = QGraphicsRectItem(0, 30, self.config.width, 1)
        line.setBrush(QBrush(self.config.fg_color))
        self.scene.addItem(line)
        
        # Render content based on type
        renderer_class = RENDERERS.get(self.config.result_type, TextResultRenderer)
        renderer_class.render(self.scene, self.config.data, self.config)
        
        self.signals.result_rendered.emit(self.config.result_type.value)
    
    def closeEvent(self, event):
        """Handle close event."""
        if self.on_close_callback:
            self.on_close_callback()
        self.signals.closed.emit()
        super().closeEvent(event)
    
    def update_result(self, config: ResultConfig) -> None:
        """Update the displayed result."""
        self.config = config
        self.scene.clear()
        self._render_results()
        self.setGeometry(100, 100, config.width, config.height)


class GraphicsShellManager:
    """
    Manages ephemeral graphics shell instances.
    
    Handles creation, lifecycle, and cleanup of shell windows.
    """
    
    def __init__(self, parent: Optional[QWidget] = None):
        self.parent = parent
        self.active_shells: Dict[str, EphemeralGraphicsShell] = {}
    
    def show_result(self, result_type: Union[str, ResultType], data: Any,
                   title: str = "Result", shell_id: Optional[str] = None,
                   auto_close_ms: Optional[int] = None, **kwargs) -> str:
        """
        Display a result in an ephemeral shell.
        
        Args:
            result_type: Type of result visualization
            data: Data to visualize
            title: Window title
            shell_id: Optional identifier for the shell (auto-generated if None)
            auto_close_ms: Auto-close after N milliseconds (None = manual close only)
            **kwargs: Additional config options (width, height, bg_color, fg_color, theme)
        
        Returns:
            shell_id of the created/updated shell
        """
        if isinstance(result_type, str):
            result_type = ResultType(result_type)
        
        if shell_id is None:
            shell_id = f"shell_{id(data)}"
        
        config = ResultConfig(
            result_type=result_type,
            title=title,
            data=data,
            auto_close_ms=auto_close_ms,
            **{k: v for k, v in kwargs.items() if k in 
               ['width', 'height', 'bg_color', 'fg_color', 'theme']}
        )
        
        if shell_id in self.active_shells:
            # Update existing shell
            self.active_shells[shell_id].update_result(config)
        else:
            # Create new shell
            def on_close_handler():
                self.active_shells.pop(shell_id, None)
            
            shell = EphemeralGraphicsShell(config, parent=self.parent,
                                         on_close=on_close_handler)
            self.active_shells[shell_id] = shell
        
        self.active_shells[shell_id].show()
        return shell_id
    
    def close_shell(self, shell_id: str) -> bool:
        """Close a specific shell."""
        if shell_id in self.active_shells:
            self.active_shells[shell_id].close()
            return True
        return False
    
    def close_all(self) -> None:
        """Close all active shells."""
        for shell in list(self.active_shells.values()):
            shell.close()
        self.active_shells.clear()


# Example usage and testing
if __name__ == "__main__":
    app = QApplication.instance() or QApplication(sys.argv)
    
    # Create manager
    manager = GraphicsShellManager()
    
    # Example 1: Text result
    manager.show_result(
        ResultType.TEXT,
        "Hello from PySide6 Graphics Shell!\nThis is an ephemeral window.",
        title="Text Output",
        auto_close_ms=None,
        width=600,
        height=400
    )
    
    # Example 2: Chart result
    manager.show_result(
        ResultType.CHART,
        {"Python": 45, "Go": 30, "Rust": 25},
        title="Language Statistics",
        width=700,
        height=500
    )
    
    # Example 3: Table result
    manager.show_result(
        ResultType.TABLE,
        [
            ["ID", "Name", "Value"],
            ["1", "Alpha", "100"],
            ["2", "Beta", "200"],
            ["3", "Gamma", "150"],
        ],
        title="Sample Table",
        width=600,
        height=400
    )
    
    # Example 4: Tree result
    manager.show_result(
        ResultType.TREE,
        {
            "root": {
                "child1": {"subchild1": "value1"},
                "child2": ["item1", "item2", "item3"]
            }
        },
        title="Hierarchical Data",
        width=600,
        height=500
    )
    
    sys.exit(app.exec())
