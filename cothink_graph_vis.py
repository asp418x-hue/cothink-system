import sys
import graphviz
from PySide6.QtCore import QByteArray, QObject, QThread, Signal
from PySide6.QtGui import QPixmap
from PySide6.QtWidgets import (
    QApplication,
    QGraphicsPixmapItem,
    QGraphicsScene,
    QGraphicsView,
    QMainWindow,
    QVBoxLayout,
    QWidget,
)


class RedpandaConsumerThread(QThread):
"""Asynchronously reads graph state payloads from Redpanda topics."""

graph_updated = Signal(str) # Emits DOT strings to UI thread

def run(self):
# Placeholder for kafka/redpanda stream consumer loop:
# e.g., consumer = KafkaConsumer('cothink-graph-telemetry', ...)
# On message -> reconstruct UltraGraph state / DOT representation
sample_dot = """
        digraph CothinkMesh {
            rankdir=LR;
            node [shape=box, style=filled, fillcolor="#1E1E2E", fontcolor="#CDD6F4", fontname="JetBrains Mono"];
            edge [color="#89B4FA"];
            LKM_Bridge -> POSIX_Worker [label="UIO/AIO"];
            POSIX_Worker -> Redpanda [label="Stream"];
            Redpanda -> UltraGraph [label="Ingest"];
            UltraGraph -> PySide6_GUI [label="Render"];
        }
        """
self.graph_updated.emit(sample_dot)


class DotGraphViewer(QMainWindow):

def __init__(self):
super().__init__()
self.setWindowTitle("cothink-system // Real-time DotGraph Telemetry")
self.resize(1024, 768)

# UI Setup
self.central_widget = QWidget()
self.layout = QVBoxLayout(self.central_widget)
self.setCentralWidget(self.central_widget)

self.scene = QGraphicsScene()
self.view = QGraphicsView(self.scene)
self.layout.addWidget(self.view)

# Worker Thread
self.consumer_thread = RedpandaConsumerThread()
self.consumer_thread.graph_updated.connect(self.update_graph_render)
self.consumer_thread.start()

def update_graph_render(self, dot_source: str):
"""Converts DOT strings to pixmaps via Graphviz and renders in PySide6."""
try:
src = graphviz.Source(dot_source, format = "png")
png_bytes = src.pipe()

pixmap = QPixmap()
pixmap.loadFromData(QByteArray(png_bytes))

self.scene.clear()
item = QGraphicsPixmapItem(pixmap)
self.scene.addItem(item)
self.view.setSceneRect(item.boundingRect())
except Exception as e:
print(f"[!] Rendering error: {
    e
}")


if __name__ == "__main__":
app = QApplication(sys.argv)
window = DotGraphViewer()
window.show()
sys.exit(app.exec())