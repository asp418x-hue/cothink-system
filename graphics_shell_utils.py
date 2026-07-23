"""
Graphics Shell Integration Utilities

Advanced patterns for integrating EphemeralGraphicsShell with data pipelines,
async operations, and custom renderers.
"""

import asyncio
import json
from typing import Any, Callable, Dict, Optional, Coroutine
from pathlib import Path
from dataclasses import asdict

from PySide6.QtCore import QThread, Signal, Slot, QObject
from PySide6.QtGui import QColor

from graphics_shell import (
    EphemeralGraphicsShell, GraphicsShellManager, ResultConfig,
    ResultRenderer, ResultType, RENDERERS
)


class AsyncShellRenderer(QObject):
    """
    Renders results asynchronously to avoid blocking the UI.
    """
    
    result_ready = Signal(ResultConfig)
    error_occurred = Signal(str)
    
    def __init__(self):
        super().__init__()
        self.worker_thread = None
    
    async def render_async(self, renderer_func: Callable[[], Coroutine],
                          result_type: ResultType, title: str) -> ResultConfig:
        """
        Render asynchronously.
        
        Args:
            renderer_func: Async function that returns data to display
            result_type: Type of result
            title: Window title
        
        Returns:
            ResultConfig ready to display
        """
        try:
            data = await renderer_func()
            config = ResultConfig(
                result_type=result_type,
                title=title,
                data=data
            )
            self.result_ready.emit(config)
            return config
        except Exception as e:
            error_msg = f"Render error: {str(e)}"
            self.error_occurred.emit(error_msg)
            raise


class CustomResultRenderer(ResultRenderer):
    """
    Template for implementing custom renderers.
    
    Usage:
        class MyCustomRenderer(CustomResultRenderer):
            @staticmethod
            def render(scene, data, config):
                # Your custom rendering logic
                pass
    """
    
    @staticmethod
    def render(scene, data, config):
        raise NotImplementedError("Subclasses must implement render()")


class JSONResultRenderer(CustomResultRenderer):
    """Renders formatted JSON with syntax highlighting simulation."""
    
    @staticmethod
    def render(scene, data, config):
        from graphics_shell import QGraphicsTextItem, QFont
        
        if isinstance(data, str):
            try:
                data = json.loads(data)
            except json.JSONDecodeError:
                pass
        
        json_str = json.dumps(data, indent=2)
        lines = json_str.split('\n')
        
        y = 40
        for line in lines[:50]:  # Limit lines
            text_item = QGraphicsTextItem(line)
            text_item.setFont(QFont("Courier", 9))
            
            # Simple syntax highlighting based on content
            if ':' in line:
                text_item.setDefaultTextColor(QColor(0, 100, 200))  # Keys in blue
            elif line.strip().startswith('"'):
                text_item.setDefaultTextColor(QColor(0, 128, 0))    # Strings in green
            elif any(c in line for c in '[]{}'):
                text_item.setDefaultTextColor(QColor(150, 0, 0))    # Braces in red
            else:
                text_item.setDefaultTextColor(config.fg_color)
            
            scene.addItem(text_item)
            text_item.setPos(30, y)
            y += 18


class DataFrameRenderer(CustomResultRenderer):
    """Renders pandas-like dataframe structures."""
    
    @staticmethod
    def render(scene, data, config):
        from graphics_shell import QGraphicsTextItem, QGraphicsRectItem, QFont, QPen, QBrush
        
        # Handle pandas DataFrame
        try:
            import pandas as pd
            if isinstance(data, pd.DataFrame):
                data = data.to_dict('records')
        except ImportError:
            pass
        
        rows = []
        if isinstance(data, list) and all(isinstance(r, dict) for r in data):
            headers = list(data[0].keys()) if data else []
            rows = [[headers]] + [[r.get(h, "") for h in headers] for r in data]
        
        col_width = max(60, (config.width - 40) // max(len(rows[0]) if rows else 1, 1))
        row_height = 25
        x, y = 20, 40
        
        for row_idx, row in enumerate(rows[:30]):
            is_header = row_idx == 0
            for col_idx, cell in enumerate(row[:10]):
                cell_x = x + col_idx * col_width
                cell_y = y + row_idx * row_height
                
                # Draw cell background
                cell_bg = QGraphicsRectItem(cell_x, cell_y, col_width, row_height)
                if is_header:
                    cell_bg.setBrush(QBrush(QColor(200, 200, 200)))
                else:
                    cell_bg.setBrush(QBrush(QColor(250, 250, 250) if row_idx % 2 == 0
                                           else QColor(240, 240, 240)))
                cell_bg.setPen(QPen(config.fg_color, 1))
                scene.addItem(cell_bg)
                
                # Draw cell text
                cell_text = QGraphicsTextItem(str(cell)[:20])
                cell_text.setFont(QFont("Arial", 9 if not is_header else 10))
                if is_header:
                    cell_text.setDefaultTextColor(QColor(0, 0, 0))
                else:
                    cell_text.setDefaultTextColor(config.fg_color)
                scene.addItem(cell_text)
                cell_text.setPos(cell_x + 5, cell_y + 3)


class ShellPipeline:
    """
    Pipeline for chaining operations and displaying results.
    
    Example:
        pipeline = ShellPipeline(manager)
        (pipeline
         .load_data("data.json")
         .transform(lambda d: {k: v*2 for k,v in d.items()})
         .display(ResultType.CHART, "Doubled Values"))
    """
    
    def __init__(self, manager: GraphicsShellManager):
        self.manager = manager
        self.data = None
        self.config = None
    
    def load_data(self, source: Any) -> "ShellPipeline":
        """Load data from various sources."""
        if isinstance(source, (str, Path)):
            source = Path(source)
            if source.suffix == '.json':
                with open(source) as f:
                    self.data = json.load(f)
            elif source.suffix == '.txt':
                with open(source) as f:
                    self.data = f.read()
            else:
                raise ValueError(f"Unsupported file type: {source.suffix}")
        else:
            self.data = source
        return self
    
    def transform(self, func: Callable[[Any], Any]) -> "ShellPipeline":
        """Apply transformation to data."""
        self.data = func(self.data)
        return self
    
    def filter(self, predicate: Callable[[Any], bool]) -> "ShellPipeline":
        """Filter data based on predicate."""
        if isinstance(self.data, list):
            self.data = [item for item in self.data if predicate(item)]
        elif isinstance(self.data, dict):
            self.data = {k: v for k, v in self.data.items() if predicate(v)}
        return self
    
    def display(self, result_type: ResultType, title: str = "Result",
               shell_id: Optional[str] = None, **kwargs) -> str:
        """Display the pipeline result."""
        return self.manager.show_result(result_type, self.data, title, shell_id, **kwargs)
    
    def save(self, filepath: str) -> "ShellPipeline":
        """Save current data to file."""
        filepath = Path(filepath)
        if filepath.suffix == '.json':
            with open(filepath, 'w') as f:
                json.dump(self.data, f, indent=2)
        elif filepath.suffix == '.txt':
            with open(filepath, 'w') as f:
                f.write(str(self.data))
        return self


class ShellFactory:
    """
    Factory for creating preconfigured shells for common scenarios.
    """
    
    @staticmethod
    def create_from_file(manager: GraphicsShellManager, filepath: str,
                        result_type: Optional[ResultType] = None) -> str:
        """Create shell from file, auto-detecting type."""
        filepath = Path(filepath)
        
        if result_type is None:
            if filepath.suffix == '.json':
                result_type = ResultType.TREE
            elif filepath.suffix == '.csv':
                result_type = ResultType.TABLE
            else:
                result_type = ResultType.TEXT
        
        with open(filepath) as f:
            if result_type == ResultType.TREE:
                data = json.load(f)
            else:
                data = f.read()
        
        return manager.show_result(result_type, data, title=filepath.stem)
    
    @staticmethod
    def create_error_shell(manager: GraphicsShellManager, error: Exception) -> str:
        """Create shell for displaying errors."""
        error_data = {
            "error_type": type(error).__name__,
            "message": str(error),
            "details": repr(error)
        }
        return manager.show_result(
            ResultType.TREE,
            error_data,
            title="Error",
            bg_color=QColor(255, 240, 240),
            fg_color=QColor(200, 0, 0),
            auto_close_ms=None
        )
    
    @staticmethod
    def create_stats_shell(manager: GraphicsShellManager, data: Any,
                          title: str = "Statistics") -> str:
        """Create shell showing data statistics."""
        if isinstance(data, list):
            stats = {
                "count": len(data),
                "types": str(set(type(x).__name__ for x in data)),
            }
        elif isinstance(data, dict):
            stats = {
                "keys": len(data),
                "total_size": sum(len(str(v)) for v in data.values())
            }
        else:
            stats = {"type": type(data).__name__}
        
        return manager.show_result(ResultType.TREE, stats, title=title)


# Register custom renderers
def register_custom_renderers():
    """Register additional custom renderers."""
    RENDERERS[ResultType.CUSTOM] = JSONResultRenderer


# Example usage
if __name__ == "__main__":
    from PySide6.QtWidgets import QApplication
    import sys
    
    app = QApplication.instance() or QApplication(sys.argv)
    manager = GraphicsShellManager()
    register_custom_renderers()
    
    # Example 1: Pipeline usage
    pipeline = ShellPipeline(manager)
    pipeline.load_data({"a": 10, "b": 20, "c": 30}).transform(
        lambda d: {k: v * 2 for k, v in d.items()}
    ).display(ResultType.CHART, "Doubled Values")
    
    # Example 2: Error display
    try:
        raise ValueError("Something went wrong!")
    except Exception as e:
        ShellFactory.create_error_shell(manager, e)
    
    # Example 3: JSON rendering
    json_data = {"status": "success", "data": [1, 2, 3], "metadata": {"version": "1.0"}}
    manager.show_result(ResultType.CUSTOM, json_data, title="JSON Output")
    
    sys.exit(app.exec())
