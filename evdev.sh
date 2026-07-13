#!/bin/bash
################################################################################
# evdev Input Device Handler
# Low-level Linux input event device manipulation
################################################################################

set -euo pipefail

# Configuration
EVDEV_PATH="${EVDEV_PATH:-/dev/input}"
EVENT_TIMEOUT="${EVENT_TIMEOUT:-5}"
DEBUG="${DEBUG:-0}"

################################################################################
# Logging Functions
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

list_input_devices() {
    local filter="${1:-}"
    
    log_debug "Listing input devices"
    
    if [[ -d "$EVDEV_PATH" ]]; then
        for dev in "$EVDEV_PATH"/event*; do
            if [[ -e "$dev" ]]; then
                local dev_name
                dev_name=$(cat "/sys/class/input/$(basename "$dev")/device/name" 2>/dev/null || echo "Unknown")
                
                if [[ -z "$filter" ]] || [[ "$dev_name" == *"$filter"* ]]; then
                    echo "$dev:$dev_name"
                fi
            fi
        done
    fi
}

get_device_info() {
    local device="$1"
    
    log_debug "Getting device info: $device"
    
    if [[ ! -e "$device" ]]; then
        log_error "Device not found: $device"
        return 1
    fi
    
    local dev_name dev_path dev_type dev_capabilities
    
    dev_path=$(readlink -f "$device")
    dev_name=$(cat "/sys/class/input/$(basename "$device")/device/name" 2>/dev/null || echo "Unknown")
    dev_type=$(cat "/sys/class/input/$(basename "$device")/name" 2>/dev/null || echo "Unknown")
    
    # Get device capabilities
    dev_capabilities=$(cat "/sys/class/input/$(basename "$device")/device/capabilities" 2>/dev/null || echo "unknown")
    
    echo "device:$device"
    echo "path:$dev_path"
    echo "name:$dev_name"
    echo "type:$dev_type"
    echo "capabilities:$dev_capabilities"
}

################################################################################
# Event Reading
################################################################################

read_events() {
    local device="$1"
    local max_events="${2:-0}"
    local count=0
    
    log_debug "Reading events from $device (max: $max_events)"
    
    if [[ ! -e "$device" ]]; then
        log_error "Device not found: $device"
        return 1
    fi
    
    # Read binary event data
    while IFS= read -r -n 24 -t "$EVENT_TIMEOUT" event_data; do
        if [[ -n "$event_data" ]]; then
            # Parse Linux input_event structure
            # struct timeval (8 bytes) + unsigned short type (2) + code (2) + value (4)
            local time_sec time_usec type code value
            
            time_sec=$(printf '%d' "0x$(printf '%s' "$event_data" | head -c 8 | od -An -tx1 | tr -d ' ')" 2>/dev/null || echo 0)
            time_usec=$(printf '%d' "0x$(printf '%s' "$event_data" | head -c 8 | tail -c 4 | od -An -tx1 | tr -d ' ')" 2>/dev/null || echo 0)
            type=$(printf '%d' "0x$(printf '%s' "$event_data" | head -c 10 | tail -c 2 | od -An -tx1 | tr -d ' ')" 2>/dev/null || echo 0)
            code=$(printf '%d' "0x$(printf '%s' "$event_data" | head -c 12 | tail -c 2 | od -An -tx1 | tr -d ' ')" 2>/dev/null || echo 0)
            value=$(printf '%d' "0x$(printf '%s' "$event_data" | head -c 16 | tail -c 4 | od -An -tx1 | tr -d ' ')" 2>/dev/null || echo 0)
            
            echo "event:timestamp=$time_sec.$time_usec type=$type code=$code value=$value"
            
            ((count++))
            if [[ $max_events -gt 0 ]] && [[ $count -ge $max_events ]]; then
                break
            fi
        fi
    done < <(xxd -p "$device" 2>/dev/null || cat "$device")
}

################################################################################
# Event Type/Code Translation
################################################################################

get_event_type_name() {
    local type="$1"
    
    case "$type" in
        0) echo "EV_SYN" ;;
        1) echo "EV_KEY" ;;
        2) echo "EV_REL" ;;
        3) echo "EV_ABS" ;;
        4) echo "EV_MSC" ;;
        5) echo "EV_SW" ;;
        17) echo "EV_LED" ;;
        18) echo "EV_SND" ;;
        20) echo "EV_REP" ;;
        21) echo "EV_FF" ;;
        22) echo "EV_PWR" ;;
        23) echo "EV_FF_STATUS" ;;
        *) echo "UNKNOWN($type)" ;;
    esac
}

get_key_name() {
    local code="$1"
    
    case "$code" in
        0) echo "RESERVED" ;;
        1) echo "ESC" ;;
        2) echo "1" ;;
        3) echo "2" ;;
        4) echo "3" ;;
        14) echo "BACKSPACE" ;;
        15) echo "TAB" ;;
        28) echo "ENTER" ;;
        29) echo "LCTRL" ;;
        42) echo "LSHIFT" ;;
        54) echo "RSHIFT" ;;
        56) echo "LALT" ;;
        100) echo "RALT" ;;
        102) echo "HOME" ;;
        103) echo "UP" ;;
        104) echo "PAGEUP" ;;
        105) echo "LEFT" ;;
        106) echo "RIGHT" ;;
        107) echo "END" ;;
        108) echo "DOWN" ;;
        109) echo "PAGEDOWN" ;;
        110) echo "INSERT" ;;
        111) echo "DELETE" ;;
        *) echo "KEY_$code" ;;
    esac
}

################################################################################
# Event Filtering
################################################################################

filter_events_by_type() {
    local device="$1"
    local event_type="$2"
    
    log_debug "Filtering events of type: $event_type"
    
    read_events "$device" 0 | while read -r line; do
        if [[ "$line" == *"type=$event_type"* ]]; then
            echo "$line"
        fi
    done
}

filter_events_by_key() {
    local device="$1"
    local key_code="$2"
    
    log_debug "Filtering events for key: $key_code"
    
    read_events "$device" 0 | while read -r line; do
        if [[ "$line" == *"type=1"* ]] && [[ "$line" == *"code=$key_code"* ]]; then
            echo "$line"
        fi
    done
}

################################################################################
# Real-time Event Monitoring
################################################################################

monitor_device() {
    local device="$1"
    local verbose="${2:-0}"
    
    log_info "Monitoring device: $device"
    
    if [[ ! -e "$device" ]]; then
        log_error "Device not found: $device"
        return 1
    fi
    
    # Use inotifywait or poll with timeout
    timeout "$EVENT_TIMEOUT" cat "$device" 2>/dev/null | od -An -tx1 -N 24 | while read -r line; do
        if [[ -n "$line" ]]; then
            if [[ $verbose -eq 1 ]]; then
                echo "RAW: $line"
            fi
        fi
    done || true
}

################################################################################
# Event Statistics
################################################################################

get_event_stats() {
    local device="$1"
    local duration="${2:-10}"
    
    log_debug "Collecting event stats for $duration seconds"
    
    local key_events=0 rel_events=0 abs_events=0 other_events=0
    local start_time end_time
    start_time=$(date +%s)
    
    timeout "$duration" cat "$device" 2>/dev/null | od -An -tx1 -N 24 | while read -r line; do
        if [[ -n "$line" ]]; then
            ((key_events++))
        fi
    done || true
    
    end_time=$(date +%s)
    local elapsed=$((end_time - start_time))
    
    echo "duration_seconds:$elapsed"
    echo "total_events:$key_events"
    echo "events_per_second:$(( key_events / max(1, elapsed) ))"
}

################################################################################
# Device Grab/Release (exclusive access)
################################################################################

grab_device() {
    local device="$1"
    
    log_info "Grabbing device for exclusive access: $device"
    
    if [[ ! -e "$device" ]]; then
        log_error "Device not found: $device"
        return 1
    fi
    
    # Use EVIOCGRAB ioctl
    printf '\x45\x4d' | dd of="$device" bs=1 count=2 2>/dev/null || log_error "Failed to grab device"
}

release_device() {
    local device="$1"
    
    log_info "Releasing device: $device"
    
    # Release is automatic on close, but we can send explicit release
    printf '\x00\x00' | dd of="$device" bs=1 count=2 2>/dev/null || log_error "Failed to release device"
}

################################################################################
# Main
################################################################################

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    case "${1:-help}" in
        list)
            list_input_devices "${2:-}"
            ;;
        info)
            get_device_info "${2:?Device required}"
            ;;
        read)
            read_events "${2:?Device required}" "${3:-5}"
            ;;
        filter-type)
            filter_events_by_type "${2:?Device required}" "${3:?Type required}"
            ;;
        filter-key)
            filter_events_by_key "${2:?Device required}" "${3:?Key code required}"
            ;;
        monitor)
            monitor_device "${2:?Device required}" "${3:-0}"
            ;;
        stats)
            get_event_stats "${2:?Device required}" "${3:-10}"
            ;;
        grab)
            grab_device "${2:?Device required}"
            ;;
        release)
            release_device "${2:?Device required}"
            ;;
        *)
            cat << 'EOF'
evdev - Linux input device handler

Usage:
  evdev.sh list [filter]                      List input devices
  evdev.sh info <device>                      Get device information
  evdev.sh read <device> [count]              Read events from device
  evdev.sh filter-type <device> <type>        Filter events by type
  evdev.sh filter-key <device> <keycode>      Filter events by key
  evdev.sh monitor <device> [verbose]         Monitor device in real-time
  evdev.sh stats <device> [duration]          Collect event statistics
  evdev.sh grab <device>                      Grab device (exclusive access)
  evdev.sh release <device>                   Release device

Environment:
  EVDEV_PATH    Path to input devices (default: /dev/input)
  EVENT_TIMEOUT Event read timeout in seconds (default: 5)
  DEBUG         Enable debug output (default: 0)

Examples:
  evdev.sh list Keyboard
  evdev.sh info /dev/input/event0
  evdev.sh read /dev/input/event0 10
  evdev.sh monitor /dev/input/event0 1
EOF
            ;;
    esac
fi
