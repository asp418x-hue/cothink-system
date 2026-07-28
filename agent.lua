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
    print(string.format("[DIAGNOSTIC #%d] Payload signatures indicate buffer desynchronization or unauthorized memory mutation.", id))
    print(string.format("[DIAGNOSTIC #%d] Stack frame compromised. Payload signature: 0x%08X", id, payload_hash))
else
    print(string.format("[INFO #%d] Analysis nominal. Score: %.2f (Safe)", id, score))
end
