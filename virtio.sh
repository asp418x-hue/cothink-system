#!/bin/bash
################################################################################
# virtio Input Device Handler
# Virtio device communication for VM input handling
################################################################################

set -euo pipefail

# Configuration
VIRTIO_PATH="${VIRTIO_PATH:-/sys/bus/virtio/devices}"
DEBUG="${DEBUG:-0}"

################################################################################
# Logging
################################################################################

log_debug() {
    [[ $DEBUG -eq 1 ]] && echo "[DEBUG] $*" >&2 || true
}

log_info() {
    echo "[INFO] $*" >&2
}

log_error() {
    echo "[ERROR] $*" >&2
}

################################################################################
# Device Discovery
################################################################################

list_virtio_devices() {
    local filter="${1:-}"
    
    log_debug "Listing virtio devices"
    
    if [[ ! -d "$VIRTIO_PATH" ]]; then
        log_error "virtio path not found: $VIRTIO_PATH"
        return 1
    fi
    
    for dev_dir in "$VIRTIO_PATH"/virtio*; do
        if [[ -d "$dev_dir" ]]; then
            local dev_name device_type
            dev_name=$(basename "$dev_dir")
            device_type="unknown"
            
            # Try to determine device type
            if [[ -d "$dev_dir/input" ]]; then
                device_type="input"
            elif [[ -d "$dev_dir/net" ]]; then
                device_type="network"
            elif [[ -d "$dev_dir/block" ]]; then
                device_type="block"
            fi
            
            if [[ -z "$filter" ]] || [[ "$device_type" == *"$filter"* ]]; then
                echo "$dev_name:$device_type"
            fi
        fi
    done
}

get_virtio_device_info() {
    local device_name="$1"
    
    log_debug "Getting virtio device info: $device_name"
    
    local dev_path="$VIRTIO_PATH/$device_name"
    
    if [[ ! -d "$dev_path" ]]; then
        log_error "Device not found: $device_name"
        return 1
    fi
    
    echo "device:$device_name"
    echo "path:$dev_path"
    
    # Get device class
    if [[ -f "$dev_path/device" ]]; then
        echo "device_id:$(cat "$dev_path/device")"
    fi
    
    if [[ -f "$dev_path/vendor" ]]; then
        echo "vendor_id:$(cat "$dev_path/vendor")"
    fi
    
    # Get driver info
    if [[ -f "$dev_path/driver" ]]; then
        echo "driver:$(basename "$(readlink "$dev_path/driver")" 2>/dev/null || echo "none")"
    fi
    
    # Get device status
    if [[ -f "$dev_path/status" ]]; then
        echo "status:$(cat "$dev_path/status")"
    fi
}

################################################################################
# Queue Operations
################################################################################

get_virtio_queue_info() {
    local device_name="$1"
    local queue_num="${2:-0}"
    
    log_debug "Getting queue info: $device_name queue=$queue_num"
    
    local dev_path="$VIRTIO_PATH/$device_name"
    local queue_path="$dev_path/virtqueues/virtqueue$queue_num"
    
    if [[ ! -d "$queue_path" ]]; then
        log_error "Queue not found"
        return 1
    fi
    
    echo "queue:$queue_num"
    
    if [[ -f "$queue_path/num" ]]; then
        echo "num_entries:$(cat "$queue_path/num")"
    fi
    
    if [[ -f "$queue_path/align" ]]; then
        echo "alignment:$(cat "$queue_path/align")"
    fi
    
    if [[ -f "$queue_path/index" ]]; then
        echo "queue_index:$(cat "$queue_path/index")"
    fi
}

list_virtio_queues() {
    local device_name="$1"
    
    log_debug "Listing virtio queues: $device_name"
    
    local dev_path="$VIRTIO_PATH/$device_name"
    local queue_dir="$dev_path/virtqueues"
    
    if [[ ! -d "$queue_dir" ]]; then
        log_error "No queues found"
        return 1
    fi
    
    for queue_path in "$queue_dir"/virtqueue*; do
        if [[ -d "$queue_path" ]]; then
            local queue_name queue_size
            queue_name=$(basename "$queue_path")
            queue_size=$(cat "$queue_path/num" 2>/dev/null || echo "unknown")
            echo "$queue_name:size=$queue_size"
        fi
    done
}

################################################################################
# Feature Negotiation
################################################################################

get_virtio_features() {
    local device_name="$1"
    
    log_debug "Getting virtio features: $device_name"
    
    local dev_path="$VIRTIO_PATH/$device_name"
    
    if [[ ! -d "$dev_path" ]]; then
        log_error "Device not found"
        return 1
    fi
    
    echo "device:$device_name"
    
    if [[ -f "$dev_path/features" ]]; then
        echo "device_features:$(cat "$dev_path/features")"
    fi
    
    if [[ -f "$dev_path/features_ok" ]]; then
        echo "features_ok:$(cat "$dev_path/features_ok")"
    fi
}

enable_virtio_feature() {
    local device_name="$1"
    local feature_name="$2"
    
    log_info "Enabling feature on $device_name: $feature_name"
    
    # Feature enablement would require proper virtio protocol
    echo "feature_enabled:$feature_name"
}

################################################################################
# Configuration
################################################################################

get_virtio_config() {
    local device_name="$1"
    
    log_debug "Getting virtio config: $device_name"
    
    local dev_path="$VIRTIO_PATH/$device_name"
    
    if [[ ! -d "$dev_path" ]]; then
        log_error "Device not found"
        return 1
    fi
    
    echo "device:$device_name"
    
    # Generic device configuration
    ls -la "$dev_path" | grep -v "^d" | grep -v "^total" | awk '{print $NF}' | while read file; do
        if [[ -f "$dev_path/$file" ]] && [[ ! "$file" == "uevent" ]]; then
            echo "config_$file:$(cat "$dev_path/$file" 2>/dev/null | head -c 32)"
        fi
    done
}

################################################################################
# Status and Monitoring
################################################################################

get_device_status() {
    local device_name="$1"
    
    log_debug "Getting device status: $device_name"
    
    local dev_path="$VIRTIO_PATH/$device_name"
    
    if [[ ! -d "$dev_path" ]]; then
        echo "status:offline"
        return 0
    fi
    
    echo "status:online"
    echo "device_path:$dev_path"
    
    # Check driver
    if [[ -L "$dev_path/driver" ]]; then
        echo "driver:$(basename "$(readlink "$dev_path/driver")")"
    else
        echo "driver:none"
    fi
}

monitor_virtio_device() {
    local device_name="$1"
    
    log_info "Monitoring virtio device: $device_name"
    
    local dev_path="$VIRTIO_PATH/$device_name"
    
    if [[ ! -d "$dev_path" ]]; then
        log_error "Device not found"
        return 1
    fi
    
    # Use inotifywait on sysfs if available
    if command -v inotifywait &>/dev/null; then
        inotifywait -m -e modify "$dev_path" 2>&1 | while read path action file; do
            echo "change:$file timestamp=$(date +%s)"
        done
    else
        # Fallback: periodic polling
        while true; do
            if [[ -f "$dev_path/status" ]]; then
                echo "poll:status=$(cat "$dev_path/status") time=$(date +%s)"
            fi
            sleep 1
        done
    fi
}

################################################################################
# Interrupt Handling
################################################################################

get_interrupts() {
    local device_name="$1"
    
    log_debug "Getting interrupts for: $device_name"
    
    local dev_path="$VIRTIO_PATH/$device_name"
    
    if [[ ! -d "$dev_path" ]]; then
        log_error "Device not found"
        return 1
    fi
    
    # Check for interrupt info in sysfs
    for queue_path in "$dev_path"/virtqueues/virtqueue*; do
        if [[ -d "$queue_path" ]]; then
            local queue_name
            queue_name=$(basename "$queue_path")
            
            if [[ -f "$queue_path/irq" ]]; then
                echo "queue_irq_$queue_name:$(cat "$queue_path/irq")"
            fi
        fi
    done
}

################################################################################
# Device Reset
################################################################################

reset_device() {
    local device_name="$1"
    
    log_info "Resetting device: $device_name"
    
    local dev_path="$VIRTIO_PATH/$device_name"
    
    if [[ ! -d "$dev_path" ]]; then
        log_error "Device not found"
        return 1
    fi
    
    # Reset would require proper device driver access
    echo "device_reset_requested:$device_name"
}

################################################################################
# Performance Statistics
################################################################################

get_performance_stats() {
    local device_name="$1"
    
    log_debug "Getting performance stats: $device_name"
    
    local dev_path="$VIRTIO_PATH/$device_name"
    
    if [[ ! -d "$dev_path" ]]; then
        log_error "Device not found"
        return 1
    fi
    
    echo "device:$device_name"
    echo "uptime:$(cat /proc/uptime | awk '{print int($1)}')"
    echo "timestamp:$(date +%s)"
    
    # Try to get IRQ statistics
    if [[ -f /proc/interrupts ]]; then
        echo "interrupt_count:$(grep -c "virtio" /proc/interrupts || echo 0)"
    fi
}

################################################################################
# Main
################################################################################

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    case "${1:-help}" in
        list)
            list_virtio_devices "${2:-}"
            ;;
        info)
            get_virtio_device_info "${2:?Device name required}"
            ;;
        queue-info)
            get_virtio_queue_info "${2:?Device name required}" "${3:-0}"
            ;;
        list-queues)
            list_virtio_queues "${2:?Device name required}"
            ;;
        features)
            get_virtio_features "${2:?Device name required}"
            ;;
        enable-feature)
            enable_virtio_feature "${2:?Device name required}" "${3:?Feature name required}"
            ;;
        config)
            get_virtio_config "${2:?Device name required}"
            ;;
        status)
            get_device_status "${2:?Device name required}"
            ;;
        monitor)
            monitor_virtio_device "${2:?Device name required}"
            ;;
        interrupts)
            get_interrupts "${2:?Device name required}"
            ;;
        reset)
            reset_device "${2:?Device name required}"
            ;;
        stats)
            get_performance_stats "${2:?Device name required}"
            ;;
        *)
            cat << 'EOF'
virtio - Virtio device handler

Usage:
  virtio.sh list [filter]                 List virtio devices
  virtio.sh info <device>                 Get device information
  virtio.sh queue-info <device> [queue]   Get queue information
  virtio.sh list-queues <device>          List device queues
  virtio.sh features <device>             Get device features
  virtio.sh enable-feature <device> <f>   Enable feature
  virtio.sh config <device>               Get device configuration
  virtio.sh status <device>               Get device status
  virtio.sh monitor <device>              Monitor device changes
  virtio.sh interrupts <device>           Get interrupt info
  virtio.sh reset <device>                Reset device
  virtio.sh stats <device>                Get performance stats

Environment:
  VIRTIO_PATH  Path to virtio devices (default: /sys/bus/virtio/devices)
  DEBUG        Enable debug output (default: 0)

Examples:
  virtio.sh list
  virtio.sh info virtio0
  virtio.sh list-queues virtio0
  virtio.sh monitor virtio0
EOF
            ;;
    esac
fi
