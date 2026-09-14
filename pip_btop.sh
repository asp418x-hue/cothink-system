#!/data/data/com.termux/files/usr/bin/bash

case "${1:-}" in
    setup|start)
        echo "Launching native htop monitor..."
        # Runs directly in terminal without X11/root dependencies
        exec htop
        ;;
    stop)
        echo "Stopping monitor..."
        pkill htop || true
        ;;
    *)
        echo "Usage: $0 {setup|start|stop}"
        exit 1
        ;;
esac
