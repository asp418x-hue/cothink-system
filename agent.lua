-- Lua classification agent
local input = io.read()
if not input then return end

-- We expect: ID,payload (e.g. 1,42,0.95 or 1,some_json)
local comma_pos = string.find(input, ",")
if not comma_pos then return end

local id_str = string.sub(input, 1, comma_pos - 1)
local payload = string.sub(input, comma_pos + 1)

local id = tonumber(id_str) or 0

-- Compute a mock hash/score based on the payload
local function hashString(str)
    local hash = 5381
    for i = 1, #str do
        hash = ((hash * 33) + string.byte(str, i)) % 4294967296
    end
    return hash
end

local payload_hash = hashString(payload)
local score = (payload_hash % 100) / 100.0

-- Random jitter based on ID to simulate distributed evaluation differences
score = score + ((id % 10) / 100.0)
if score > 1.0 then score = 1.0 end

local threshold = score > 0.65

print(string.format("[Rust Agent #%d] Intercepted payload stream.", id))
print(string.format("[Rust Agent #%d] Buffer size: %d bytes", id, #payload))

if threshold then
    print(string.format("[CRITICAL #%d] Anomaly Score: %.2f (Threshold Exceeded!)", id, score))
    
    print(string.format("[DIAGNOSTIC #%d] Fetching real system state for debugging...", id))
    
    -- Gather real system load and memory info
    local function getSysCommandOutput(cmd)
        local f = io.popen(cmd)
        if not f then return "N/A" end
        local result = f:read("*a")
        f:close()
        return result
    end
    
    local load_avg = getSysCommandOutput("cat /proc/loadavg")
    local mem_info = getSysCommandOutput("free -m | grep Mem")
    
    print(string.format("[DIAGNOSTIC #%d] Load Avg: %s", id, load_avg:gsub("\n", "")))
    print(string.format("[DIAGNOSTIC #%d] Memory (MB): %s", id, mem_info:gsub("\n", "")))
    
    -- Print out the exact payload that caused it
    print(string.format("[DIAGNOSTIC #%d] Faulting Payload: %s", id, payload))
else
    print(string.format("[INFO #%d] Telemetry nominal. Score: %.2f (Safe)", id, score))
end
