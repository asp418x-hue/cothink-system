#!/usr/bin/env python3
import sys
import json
import os
import time

import urllib.request
import urllib.error

def process_task(task_id, context):
    # Check for API key in environment
    api_key = os.environ.get("GEMINI_API_KEY") or os.environ.get("GOOGLE_API_KEY")
    
    if not api_key:
        # Fallback to mock if no key is provided
        time.sleep(0.5)
        result = {
            "task_id": task_id,
            "status": "success",
            "provider": "mock-provider",
            "resolution": f"LLM generated response for task {task_id}",
            "raw_context": context,
            "note": "No API key found in GEMINI_API_KEY or GOOGLE_API_KEY, returned mock."
        }
        return json.dumps(result)
        
    url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={api_key}"
    prompt = f"Please act as an autonomous agent and process the following task.\nTask ID: {task_id}\nContext: {context}"
    payload = {"contents": [{"parts": [{"text": prompt}]}]}
    data = json.dumps(payload).encode("utf-8")
    headers = {"Content-Type": "application/json"}
    
    req = urllib.request.Request(url, data=data, headers=headers)
    
    try:
        with urllib.request.urlopen(req) as response:
            result_json = json.loads(response.read().decode("utf-8"))
            text = result_json.get("candidates", [{}])[0].get("content", {}).get("parts", [{}])[0].get("text", "")
            
            return json.dumps({
                "task_id": task_id,
                "status": "success",
                "provider": "gemini",
                "resolution": text.strip(),
                "raw_context": context
            })
    except urllib.error.HTTPError as e:
        return json.dumps({
            "task_id": task_id,
            "status": "error",
            "provider": "gemini",
            "error_msg": str(e),
            "details": e.read().decode("utf-8")
        })
    except Exception as e:
        return json.dumps({
            "task_id": task_id,
            "status": "error",
            "provider": "gemini",
            "error_msg": str(e)
        })

def main():
    if len(sys.argv) < 2:
        print(json.dumps({"error": "Missing task_id argument"}), file=sys.stderr)
        sys.exit(1)
        
    try:
        task_id = int(sys.argv[1])
    except ValueError:
        print(json.dumps({"error": "task_id must be an integer"}), file=sys.stderr)
        sys.exit(1)
        
    # Read any context passed via stdin
    if not sys.stdin.isatty():
        context = sys.stdin.read().strip()
    else:
        context = ""
        
    try:
        response = process_task(task_id, context)
        # The rust orchestrator captures stdout
        print(response)
    except Exception as e:
        print(json.dumps({"task_id": task_id, "error": str(e)}), file=sys.stderr)
        sys.exit(1)

if __name__ == "__main__":
    main()
