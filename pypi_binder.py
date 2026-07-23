import os
import sys
import json
import socket
import threading
import time
import urllib.request
import urllib.error

class PyPIBinder:
    """
    A binder client for PyPI registry APIs.
    Authenticates dynamically by requesting OAuth tokens over a Unix domain socket.
    """
    def __init__(self, socket_path="/tmp/oauth_gateway.sock", pypi_url="https://upload.pypi.org/legacy/"):
        self.socket_path = socket_path
        self.pypi_url = pypi_url

    def _get_oauth_token(self, scope: str) -> str:
        """
        Connects to the local Unix socket gateway to fetch an OAuth token for a specific scope.
        """
        if not os.path.exists(self.socket_path):
            raise ConnectionError(f"OAuth Unix socket not found at {self.socket_path}")

        client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            client.connect(self.socket_path)
            
            # Send OAuth token request payload
            request_payload = {
                "action": "get_token",
                "scope": scope,
                "timestamp": int(time.time())
            }
            client.sendall(json.dumps(request_payload).encode('utf-8'))

            # Read response
            response_data = client.recv(4096)
            if not response_data:
                raise ValueError("Empty response received from OAuth socket")

            response = json.loads(response_data.decode('utf-8'))
            if response.get("status") != "success":
                raise PermissionError(f"OAuth request failed: {response.get('error')}")

            token = response.get("token")
            print(f"[PyPIBinder - Unix Socket] Successfully acquired OAuth token for scope '{scope}'")
            return token

        except Exception as e:
            raise RuntimeError(f"Unix socket OAuth handshake failed: {e}")
        finally:
            client.close()

    def get_package_info(self, package_name: str) -> dict:
        """
        Subroutine: Retrieve metadata for a specific package from PyPI.
        Uses Unix socket OAuth token for the 'read' scope.
        """
        print(f"\n[PyPIBinder] Subroutine: get_package_info for '{package_name}'")
        token = self._get_oauth_token(scope="read")
        
        # Build API request (using a mock registry URL or pypi.org API)
        url = f"https://pypi.org/pypi/{package_name}/json"
        
        req = urllib.request.Request(url)
        req.add_header("Authorization", f"Bearer {token}")
        req.add_header("User-Agent", "AntigravityPyPIBinder/1.0")

        try:
            with urllib.request.urlopen(req) as response:
                print("[PyPIBinder] API Request successfully authenticated.")
                return json.loads(response.read().decode('utf-8'))
        except urllib.error.HTTPError as e:
            if e.code == 404:
                print(f"[PyPIBinder] Package '{package_name}' not found on PyPI (expected).")
                return {"error": "Not Found"}
            raise e
        except Exception as e:
            print(f"[PyPIBinder] API request failed: {e}")
            return {"error": str(e)}

    def publish_package(self, package_name: str, version: str, tarball_data: bytes) -> bool:
        """
        Subroutine: Upload/publish a package release to PyPI.
        Uses Unix socket OAuth token for the 'write' scope.
        """
        print(f"\n[PyPIBinder] Subroutine: publish_package for '{package_name} v{version}'")
        token = self._get_oauth_token(scope="write")

        # In a real environment, this makes a multipart POST request to self.pypi_url.
        # Since this is a demo/test run, we mock the final HTTP call but show authorization headers.
        print(f"[PyPIBinder] Simulating package upload to {self.pypi_url}...")
        print(f"[PyPIBinder] Authorization Header: Bearer {token[:12]}...{token[-4:]}")
        print(f"[PyPIBinder] Payload: Sending {len(tarball_data)} bytes of package tarball.")
        time.sleep(0.5)
        print("[PyPIBinder] Package upload mock completed successfully!")
        return True


# ==========================================
# MOCK UNIX SOCKET OAUTH SERVER FOR TESTING
# ==========================================
class MockOAuthServer:
    def __init__(self, socket_path="/tmp/oauth_gateway.sock"):
        self.socket_path = socket_path
        self.running = False
        self.server_socket = None

    def start(self):
        # Cleanup existing socket if any
        if os.path.exists(self.socket_path):
            os.remove(self.socket_path)

        self.server_socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.server_socket.bind(self.socket_path)
        self.server_socket.listen(5)
        self.running = True
        
        self.thread = threading.Thread(target=self._run_loop, daemon=True)
        self.thread.start()
        print(f"[Mock OAuth Server] Listening on Unix socket: {self.socket_path}")

    def _run_loop(self):
        while self.running:
            try:
                conn, _ = self.server_socket.accept()
                data = conn.recv(1024)
                if data:
                    req = json.loads(data.decode('utf-8'))
                    scope = req.get("scope", "default")
                    
                    # Generate mock token
                    response = {
                        "status": "success",
                        "token": f"pypi_oauth_tkn_xyz_{scope}_998877abc",
                        "expires_in": 3600
                    }
                    conn.sendall(json.dumps(response).encode('utf-8'))
                conn.close()
            except Exception:
                break

    def stop(self):
        self.running = False
        if self.server_socket:
            self.server_socket.close()
        if os.path.exists(self.socket_path):
            os.remove(self.socket_path)
        print("[Mock OAuth Server] Stopped and socket cleaned up.")


if __name__ == "__main__":
    socket_path = "/tmp/oauth_gateway.sock"
    
    # Initialize and start Mock Server
    server = MockOAuthServer(socket_path)
    server.start()

    try:
        # Initialize client binder
        binder = PyPIBinder(socket_path=socket_path)

        # 1. Test package info retrieval subroutine (Reads from public PyPI API)
        # Note: We query a package that definitely exists like 'urllib3'
        info = binder.get_package_info("urllib3")
        if "info" in info:
            print(f"[Success] Package: {info['info'].get('name')}")
            print(f"[Success] Latest Version: {info['info'].get('version')}")
        else:
            print("[Info] Returned data:", info)

        # 2. Test publishing package subroutine (Writes using simulated API)
        tarball = b"Gzipped tarball simulated contents for cothink-agent-package"
        success = binder.publish_package("cothink-agent-pkg", "1.0.0", tarball)
        print(f"[Success] Package publish status: {success}")

    finally:
        # Clean up
        server.stop()
