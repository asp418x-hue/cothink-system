import 'dart:io';
import 'dart:async';
import 'package:flutter/foundation.dart';

class RuntimeManager {
  Process? _goServerProcess;
  Process? _rustThermalProcess;
  Process? _rustClassifierProcess;
  
  static final RuntimeManager _instance = RuntimeManager._internal();
  factory RuntimeManager() => _instance;
  RuntimeManager._internal();

  Future<void> startRuntimes() async {
    try {
      debugPrint('Starting Go json_server...');
      _goServerProcess = await Process.start(
        'go',
        ['run', 'main/json_server.go'],
        runInShell: true,
      );
      _goServerProcess?.stdout.listen((data) => debugPrint('Go: ${String.fromCharCodes(data)}'));
      _goServerProcess?.stderr.listen((data) => debugPrint('Go Error: ${String.fromCharCodes(data)}'));

      debugPrint('Starting Rust thermal_monitor...');
      _rustThermalProcess = await Process.start(
        'cargo',
        ['run', '--bin', 'thermal_monitor'],
        runInShell: true,
      );
      _rustThermalProcess?.stdout.listen((data) => debugPrint('Rust Thermal: ${String.fromCharCodes(data)}'));
      _rustThermalProcess?.stderr.listen((data) => debugPrint('Rust Thermal Error: ${String.fromCharCodes(data)}'));

      debugPrint('Starting Rust classifier...');
      _rustClassifierProcess = await Process.start(
        'cargo',
        ['run', '--bin', 'classifier'],
        runInShell: true,
      );
      _rustClassifierProcess?.stdout.listen((data) => debugPrint('Rust Classifier: ${String.fromCharCodes(data)}'));
      _rustClassifierProcess?.stderr.listen((data) => debugPrint('Rust Classifier Error: ${String.fromCharCodes(data)}'));

    } catch (e) {
      debugPrint('Error starting runtimes: $e');
    }
  }

  void stopRuntimes() {
    debugPrint('Stopping runtimes...');
    _goServerProcess?.kill();
    _rustThermalProcess?.kill();
    _rustClassifierProcess?.kill();
  }
}
