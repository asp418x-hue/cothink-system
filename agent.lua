-- Lua classification agent
local input = io.read()
if not input then return end

local id, raw_val = input:match("([^,]+),([^,]+)")
id = tonumber(id)
raw_val = tonumber(raw_val)

local score = math.min(raw_val, 1.0)
local threshold = score > 0.5

print(string.format("[Rust Agent] Reading frame ID %d...", id))
print("[Rust Agent] Payload size: 5 bytes")
print(string.format("[Rust Agent] Analysis anomaly score: %.2f (Threshold crossed: %s)", score, tostring(threshold)))
