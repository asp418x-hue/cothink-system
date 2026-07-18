#!/bin/bash
################################################################################
# hidraw HID Raw Interface Handler
# USB HID device communication
################################################################################

set -euo pipefail

# Configuration
HIDRAW_PATH="${HIDRAW_PATH:-/dev/hidraw}"
HIDRAW_TIMEOUT="${HIDRAW_TIMEOUT:-5}"
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

list_hidraw_devices() {
    local filter="${1:-}"
    
    log_debug "Listing HID raw devices"
    
    for dev in /dev/hidraw*; do
        if [[ -e "$dev" ]]; then
            # Get device info from sysfs
            local devnum device_info
            devnum=$(basename "$dev" | sed 's/hidraw//')
            device_info=$(cat "/sys/class/hidraw/hidraw$devnum/device/uevent" 2>/dev/null || echo "")
            
            if [[ -z "$filter" ]] || [[ "$device_info" == *"$filter"* ]]; then
                echo "$dev:$device_info"
            fi
        fi
    done
}

get_hidraw_info() {
    local device="$1"
    
    log_debug "Getting HID device info: $device"
    
    if [[ ! -e "$device" ]]; then
        log_error "Device not found: $device"
        return 1
    fi
    
    local devnum sysfs_path
    devnum=$(basename "$device" | sed 's/hidraw//')
    sysfs_path="/sys/class/hidraw/hidraw$devnum"
    
    if [[ -d "$sysfs_path" ]]; then
        echo "device:$device"
        echo "devnum:$devnum"
        
        # Read from device/uevent
        if [[ -f "$sysfs_path/device/uevent" ]]; then
            cat "$sysfs_path/device/uevent" | sed 's/^/  /'
        fi
        
        # Try to get vendor/product from parent device
        if [[ -f "$sysfs_path/device/../idVendor" ]]; then
            local vendor product
            vendor=$(cat "$sysfs_path/device/../idVendor" 2>/dev/null || echo "0000")
            product=$(cat "$sysfs_path/device/../idProduct" 2>/dev/null || echo "0000")
            echo "vendor_id:$vendor"
            echo "product_id:$product"
        fi
    fi
}

################################################################################
# HID Report Operations
################################################################################

get_hid_report_descriptor() {
    local device="$1"
    
    log_debug "Reading HID report descriptor: $device"
    
    if [[ ! -e "$device" ]]; then
        log_error "Device not found: $device"
        return 1
    fi
    
    local devnum sysfs_path report_path
    devnum=$(basename "$device" | sed 's/hidraw//')
    sysfs_path="/sys/class/hidraw/hidraw$devnum"
    report_path="$sysfs_path/report_descriptor"
    
    if [[ -f "$report_path" ]]; then
        xxd -p "$report_path"
    else
        log_error "Report descriptor not found"
        return 1
    fi
}

send_hid_report() {
    local device="$1"
    local report_id="$2"
    shift 2
    local data="$@"
    
    log_debug "Sending HID report to $device: $data"
    
    if [[ ! -e "$device" ]]; then
        log_error "Device not found: $device"
        return 1
    fi
    
    # Construct report: report_id + data
    local report
    report=$(printf '%02x' "$report_id")
    for byte in $data; do
        report="$report$(printf '%02x' "$byte")"
    done
    
    # Write to device
    echo -ne "$(printf '%s' "$report" | sed 's/../\\x&/g')" > "$device" || {
        log_error "Failed to send report"
        return 1
    }
    
    echo "report_sent:$report"
}

read_hid_report() {
    local device="$1"
    local timeout="${2:-$HIDRAW_TIMEOUT}"
    
    log_debug "Reading HID report from $device"
    
    if [[ ! -e "$device" ]]; then
        log_error "Device not found: $device"
        return 1
    fi
    
    # Read with timeout
    timeout "$timeout" xxd -p "$device" | head -c 256 || true
    echo
}

################################################################################
# HID Feature Reports
################################################################################

get_hid_feature() {
    local device="$1"
    local report_id="$2"
    
    log_debug "Getting HID feature report: $report_id"
    
    # Note: This requires ioctl access which is complex in bash
    # We'll use hid-tools if available
    if command -v hidtool &>/dev/null; then
        hidtool get_feature "$device" "$report_id" 2>/dev/null || {
            log_error "Failed to get feature report"
            return 1
        }
    else
        log_error "hidtool not available"
        return 1
    fi
}

set_hid_feature() {
    local device="$1"
    local report_id="$2"
    shift 2
    local data="$@"
    
    log_debug "Setting HID feature report: $data"
    
    if command -v hidtool &>/dev/null; then
        hidtool set_feature "$device" "$report_id" $data 2>/dev/null || {
            log_error "Failed to set feature report"
            return 1
        }
    else
        log_error "hidtool not available"
        return 1
    fi
}

################################################################################
# Device Communication
################################################################################

open_device() {
    local device="$1"
    local fd="${2:-3}"
    
    log_debug "Opening device: $device (fd: $fd)"
    
    if [[ ! -e "$device" ]]; then
        log_error "Device not found: $device"
        return 1
    fi
    
    eval "exec $fd<>$device"
    log_info "Device opened on fd $fd"
}

close_device() {
    local fd="${1:-3}"
    
    log_debug "Closing device (fd: $fd)"
    eval "exec $fd>&-"
}

write_to_device() {
    local fd="$1"
    shift
    local data="$@"
    
    log_debug "Writing to fd $fd: $data"
    
    echo -ne "$(printf '%s' "$data" | sed 's/../\\x&/g')" >&"$fd" || {
        log_error "Write failed"
        return 1
    }
}

read_from_device() {
    local fd="$1"
    local length="${2:-64}"
    
    log_debug "Reading from fd $fd (length: $length)"
    
    dd if="/proc/self/fd/$fd" bs="$length" count=1 2>/dev/null | xxd -p || true
}

################################################################################
# Device Status
################################################################################

get_device_status() {
    local device="$1"
    
    log_debug "Getting device status: $device"
    
    if [[ ! -e "$device" ]]; then
        echo "status:offline"
        return 0
    fi
    
    # Check if device is accessible
    if [[ -r "$device" ]] && [[ -w "$device" ]]; then
        echo "status:online"
        echo "readable:1"
        echo "writable:1"
    else
        echo "status:limited"
        echo "readable:$(([[ -r "$device" ]] && echo 1 || echo 0))"
        echo "writable:$(([[ -w "$device" ]] && echo 1 || echo 0))"
    fi
    
    # Get device permissions
    ls -l "$device" | awk '{print "permissions:" $1}'
}

################################################################################
# Device Monitoring
################################################################################

monitor_hidraw() {
    local device="$1"
    
    log_info "Monitoring HID device: $device"
    
    if [[ ! -e "$device" ]]; then
        log_error "Device not found: $device"
        return 1
    fi
    
    # Monitor with udevadm
    if command -v udevadm &>/dev/null; then
        udevadm monitor --property --udev 2>&1 | grep -i "hidraw\|$device" || true
    else
        # Fallback: poll device
        while true; do
            if [[ -e "$device" ]]; then
                echo "device:online"
            else
                echo "device:offline"
            fi
            sleep 1
        done
    fi
}

################################################################################
# Protocol Helpers
################################################################################

parse_hid_report() {
    local report_hex="$1"
    
    log_debug "Parsing HID report: $report_hex"
    
    # Split into bytes
    echo "$report_hex" | sed 's/../& /g' | awk '{
        for(i=1; i<=NF; i++) {
            printf "byte_%d:0x%s\n", i-1, $i
        }
    }'
}

construct_hid_report() {
    local report_id="$1"
    shift
    local bytes="$@"
    
    log_debug "Constructing HID report: id=$report_id data=$bytes"
    
    printf '%02x' "$report_id"
    for byte in $bytes; do
        printf '%02x' "$byte"
    done
    echo
}

################################################################################
# Main
################################################################################

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    case "${1:-help}" in
        list)
            list_hidraw_devices "${2:-}"
            ;;
        info)
            get_hidraw_info "${2:?Device required}"
            ;;
        descriptor)
            get_hid_report_descriptor "${2:?Device required}"
            ;;
        send)
            send_hid_report "${2:?Device required}" "${3:?Report ID required}" "${@:4}"
            ;;
        read)
            read_hid_report "${2:?Device required}" "${3:-$HIDRAW_TIMEOUT}"
            ;;
        status)
            get_device_status "${2:?Device required}"
            ;;
        monitor)
            monitor_hidraw "${2:?Device required}"
            ;;
        open)
            open_device "${2:?Device required}" "${3:-3}"
            ;;
        close)
            close_device "${2:-3}"
            ;;
        write)
            write_to_device "${2:?FD required}" "${@:3}"
            ;;
        read-fd)
            read_from_device "${2:?FD required}" "${3:-64}"
            ;;
        parse)
            parse_hid_report "${2:?Report hex required}"
            ;;
        construct)
            construct_hid_report "${2:?Report ID required}" "${@:3}"
            ;;
        *)
            cat << 'EOF'
hidraw - USB HID raw interface handler

Usage:
  hidraw.sh list [filter]                        List HID devices
  hidraw.sh info <device>                        Get device information
  hidraw.sh descriptor <device>                  Get HID report descriptor
  hidraw.sh send <device> <id> <bytes...>      Send HID report
  hidraw.sh read <device> [timeout]              Read HID report
  hidraw.sh status <device>                      Get device status
  hidraw.sh monitor <device>                     Monitor device
  hidraw.sh open <device> [fd]                   Open device (default: fd=3)
  hidraw.sh close [fd]                           Close device (default: fd=3)
  hidraw.sh write <fd> <data>                    Write to device
  hidraw.sh read-fd <fd> [length]                Read from device
  hidraw.sh parse <hex>                          Parse HID report
  hidraw.sh construct <id> <bytes...>           Construct HID report

Environment:
  HIDRAW_PATH     Path to HID devices (default: /dev/hidraw)
  HIDRAW_TIMEOUT  Read timeout in seconds (default: 5)
  DEBUG           Enable debug output (default: 0)

Examples:
  hidraw.sh list Keyboard
  hidraw.sh info /dev/hidraw0
  hidraw.sh send /dev/hidraw0 0 65 66 67
  hidraw.sh monitor /dev/hidraw0
EOF
            ;;
    esac
fi
