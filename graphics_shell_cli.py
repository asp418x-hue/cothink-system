#!/usr/bin/env python3
"""
Graphics Shell CLI - Command-line interface for the ephemeral graphics shell.

Usage:
    graphics_shell.py --type text --title "Output" --data "Hello World"
    graphics_shell.py --type chart --data '{"A": 10, "B": 20}'
    graphics_shell.py --type table --file data.json
    graphics_shell.py --type tree --stdin
"""

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Optional

from PySide6.QtWidgets import QApplication
from PySide6.QtGui import QColor

from graphics_shell import ResultType, GraphicsShellManager


def parse_color(color_str: str) -> Optional[QColor]:
    """Parse color from string (hex or rgb)."""
    if not color_str:
        return None
    
    if color_str.startswith('#'):
        return QColor(color_str)
    
    try:
        parts = [int(x.strip()) for x in color_str.split(',')]
        if len(parts) == 3:
            return QColor(*parts)
        elif len(parts) == 4:
            return QColor(*parts)
    except (ValueError, AttributeError):
        pass
    
    return None


def load_data(source: Optional[str], stdin: bool = False) -> Any:
    """Load data from various sources."""
    if stdin:
        data_str = sys.stdin.read()
        try:
            return json.loads(data_str)
        except json.JSONDecodeError:
            return data_str
    
    if source is None:
        return None
    
    # Try parsing as JSON
    try:
        return json.loads(source)
    except json.JSONDecodeError:
        pass
    
    # Try loading from file
    filepath = Path(source)
    if filepath.exists():
        with open(filepath) as f:
            if filepath.suffix == '.json':
                return json.load(f)
            else:
                return f.read()
    
    # Return as plain string
    return source


def parse_args():
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser(
        description="Display results in an ephemeral PySide6 graphics shell",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Text output
  graphics_shell.py --type text --title "Error" --data "Connection failed"
  
  # Chart from JSON
  graphics_shell.py --type chart --data '{"Python": 45, "Go": 30}'
  
  # Table from file
  graphics_shell.py --type table --file data.json
  
  # Tree from stdin
  echo '{"key": {"nested": "value"}}' | graphics_shell.py --type tree --stdin
  
  # Grid visualization
  graphics_shell.py --type grid --data '[[1,2,3],[4,5,6],[7,8,9]]'
  
  # Custom theme
  graphics_shell.py --type text --data "Dark mode" --theme dark --bg-color 30,30,30 --fg-color 255,255,255
        """
    )
    
    # Basic arguments
    parser.add_argument(
        '--type', '-t',
        type=str,
        choices=['text', 'chart', 'table', 'tree', 'grid', 'custom'],
        default='text',
        help='Type of result visualization'
    )
    
    parser.add_argument(
        '--title',
        type=str,
        default='Result',
        help='Window title'
    )
    
    parser.add_argument(
        '--id',
        type=str,
        help='Shell identifier (for reusing windows)'
    )
    
    # Data input
    parser.add_argument(
        '--data', '-d',
        type=str,
        help='Data to display (JSON, plain text, or file path)'
    )
    
    parser.add_argument(
        '--file', '-f',
        type=str,
        help='Load data from file'
    )
    
    parser.add_argument(
        '--stdin',
        action='store_true',
        help='Read data from standard input'
    )
    
    # Window appearance
    parser.add_argument(
        '--width',
        type=int,
        default=800,
        help='Window width in pixels'
    )
    
    parser.add_argument(
        '--height',
        type=int,
        default=600,
        help='Window height in pixels'
    )
    
    parser.add_argument(
        '--bg-color',
        type=str,
        help='Background color (hex or R,G,B)'
    )
    
    parser.add_argument(
        '--fg-color',
        type=str,
        help='Foreground/text color (hex or R,G,B)'
    )
    
    parser.add_argument(
        '--theme',
        choices=['light', 'dark'],
        default='light',
        help='Color theme preset'
    )
    
    # Behavior
    parser.add_argument(
        '--auto-close',
        type=int,
        help='Auto-close after N milliseconds'
    )
    
    parser.add_argument(
        '--stay',
        action='store_true',
        help='Keep window open until manually closed'
    )
    
    # Advanced
    parser.add_argument(
        '--json-output',
        action='store_true',
        help='Output shell ID as JSON'
    )
    
    return parser.parse_args()


def main():
    """Main entry point."""
    args = parse_args()
    
    # Initialize Qt application
    app = QApplication.instance() or QApplication(sys.argv)
    
    # Load data
    if args.stdin:
        data = load_data(None, stdin=True)
    elif args.file:
        data = load_data(args.file)
    elif args.data:
        data = load_data(args.data)
    else:
        data = ""
    
    # Parse colors
    bg_color = parse_color(args.bg_color)
    fg_color = parse_color(args.fg_color)
    
    # Prepare kwargs
    kwargs = {
        'width': args.width,
        'height': args.height,
        'theme': args.theme,
    }
    
    if bg_color:
        kwargs['bg_color'] = bg_color
    if fg_color:
        kwargs['fg_color'] = fg_color
    
    # Setup auto-close
    auto_close = None
    if args.auto_close:
        auto_close = args.auto_close
    elif not args.stay:
        # Default: auto-close after 30 seconds if not --stay
        # Remove this if you want windows to stay by default
        pass
    
    # Create and show shell
    manager = GraphicsShellManager()
    shell_id = manager.show_result(
        args.type,
        data,
        title=args.title,
        shell_id=args.id,
        auto_close_ms=auto_close,
        **kwargs
    )
    
    # Output result
    if args.json_output:
        output = json.dumps({
            "shell_id": shell_id,
            "result_type": args.type,
            "title": args.title,
        })
        print(output)
    else:
        print(f"Shell opened: {shell_id}", file=sys.stderr)
    
    # Run application
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
