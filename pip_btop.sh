#!/bin/bash
# pip_btop.sh - Setup, start, and stop a Picture-in-Picture btop window for thermal monitoring.

BTOP_CONF_DIR="/home/adrian/.config/btop"
BTOP_CONF="$BTOP_CONF_DIR/btop.conf"
FLUXBOX_APPS="/home/adrian/.fluxbox/apps"

setup_btop() {
    echo "Configuring btop..."
    mkdir -p "$BTOP_CONF_DIR"
    
    # Write config to show only CPU / Thermals with a nice theme
    cat << 'EOF' > "$BTOP_CONF"
#? Config file for btop v1.3.2
color_theme="dracula"
theme_background=True
shown_boxes="cpu"
update_ms=1000
proc_info=True
cpu_single_graph=False
cpu_bottom=False
EOF
    chown -R adrian:adrian "$BTOP_CONF_DIR" || true
    
    # Configure fluxbox apps rule for btop_pip
    if ! grep -q "title=btop_pip" "$FLUXBOX_APPS" 2>/dev/null; then
        echo "Adding Fluxbox app rule for btop_pip..."
        cat << 'EOF' >> "$FLUXBOX_APPS"

[app] (title=btop_pip)
  [Dimensions]	{600 400}
  [Position]	(BOTTOMRIGHT)	{-16 -48}
  [Layer]	{2}
  [Sticky]	{yes}
  [Deco]	{NONE}
[end]
EOF
        chown adrian:adrian "$FLUXBOX_APPS" || true
        # Signal fluxbox to reload configuration if running
        if pgrep fluxbox >/dev/null; then
            DISPLAY=:0.0 fluxbox-remote Reconfigure || true
        fi
    fi
}

start_btop() {
    # If already running, don't start another
    if DISPLAY=:0.0 wmctrl -l | grep "btop_pip" &>/dev/null; then
        echo "btop_pip is already running."
        exit 0
    fi
    
    echo "Starting pip'ed btop window..."
    # Start lxterminal in background executing btop under user 'adrian' to ensure proper X session permissions
    sudo -u adrian DISPLAY=:0.0 lxterminal -t "btop_pip" -e btop &
    
    # Wait for the window to appear (up to 3 seconds)
    local win_id=""
    for i in {1..30}; do
        win_id=$(DISPLAY=:0.0 wmctrl -l | grep "btop_pip" | awk '{print $1}' | head -n 1 || true)
        if [[ -n "$win_id" ]]; then
            break
        fi
        sleep 0.1
    done
    
    if [[ -z "$win_id" ]]; then
        echo "Failed to find btop_pip window."
        exit 1
    fi
    
    echo "Positioning and styling btop_pip window (ID: $win_id)..."
    
    # Apply dynamic X11 attributes to ensure PiP behavior
    # 1. Make Always on Top (above)
    DISPLAY=:0.0 wmctrl -i -r "$win_id" -b add,above
    
    # 2. Make Sticky (visible on all workspaces)
    DISPLAY=:0.0 wmctrl -i -r "$win_id" -b add,sticky
    
    # 3. Position and Resize: x=750, y=320, width=600, height=400
    DISPLAY=:0.0 wmctrl -i -r "$win_id" -e 0,750,320,600,400
    
    # 4. Remove borders/decorations dynamically via Motif hints
    DISPLAY=:0.0 xprop -id "$win_id" -f _MOTIF_WM_HINTS 32c -set _MOTIF_WM_HINTS "0x2, 0x0, 0x0, 0x0, 0x0"
    
    # 5. Bring window to top focus / raise it
    DISPLAY=:0.0 xdotool windowraise "$win_id"
    
    echo "btop_pip started successfully."
}

stop_btop() {
    echo "Stopping btop_pip..."
    local win_id
    win_id=$(DISPLAY=:0.0 wmctrl -l | grep "btop_pip" | awk '{print $1}' || true)
    if [[ -n "$win_id" ]]; then
        for id in $win_id; do
            DISPLAY=:0.0 xdotool windowkill "$id" 2>/dev/null || true
        done
        echo "btop_pip stopped."
    else
        echo "btop_pip not running."
    fi
}

case "${1:-}" in
    setup)
        setup_btop
        ;;
    start)
        setup_btop
        start_btop
        ;;
    stop)
        stop_btop
        ;;
    *)
        echo "Usage: $0 {setup|start|stop}"
        exit 1
        ;;
esac
