import 'dart:io';

class PathUtils {
  static String? _cachedProjectRoot;

  /// Dynamically determines the root directory of the cothink-system project.
  static String get projectRoot {
    if (_cachedProjectRoot != null) {
      return _cachedProjectRoot!;
    }

    // 1. Check environment variable
    final envPath = Platform.environment['COTHINK_ROOT'];
    if (envPath != null && Directory(envPath).existsSync()) {
      _cachedProjectRoot = envPath;
      return envPath;
    }

    // 2. Search upwards from current working directory
    Directory dir = Directory.current;
    for (int i = 0; i < 5; i++) {
      if (_isProjectRoot(dir)) {
        _cachedProjectRoot = dir.path;
        return dir.path;
      }
      final parent = dir.parent;
      if (parent.path == dir.path) break;
      dir = parent;
    }

    // 3. Check known workspace locations
    const knownLocations = [
      '/home/adrian/Documents/GitHub/cothink-system',
      '/home/asp418x/cothink-system',
    ];

    for (final loc in knownLocations) {
      if (Directory(loc).existsSync()) {
        _cachedProjectRoot = loc;
        return loc;
      }
    }

    // 4. Default fallback to current directory
    _cachedProjectRoot = Directory.current.path;
    return _cachedProjectRoot!;
  }

  static bool _isProjectRoot(Directory dir) {
    return File('${dir.path}/bin/json_server').existsSync() ||
        File('${dir.path}/bin/classifier').existsSync() ||
        File('${dir.path}/pip_btop.sh').existsSync() ||
        File('${dir.path}/Cargo.toml').existsSync();
  }

  /// Returns the absolute path to a binary in the project bin directory.
  static String binaryPath(String binaryName) {
    return '$projectRoot/bin/$binaryName';
  }

  /// Returns the absolute path to a script in the project root directory.
  static String scriptPath(String scriptName) {
    return '$projectRoot/$scriptName';
  }
}
