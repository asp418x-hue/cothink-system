import 'dart:io';
import 'dart:async';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
class RuntimeManager {
  Process? _goServerProcess;
  Process? _rustThermalProcess;
  Process? _rustClassifierProcess;
  
  static final RuntimeManager _instance = RuntimeManager._internal();
  factory RuntimeManager() => _instance;
  RuntimeManager._internal();

  Future<void> _extractAndRunAndroid() async {
    final supportDir = await getApplicationSupportDirectory();
    
    Future<Process> extractAndRun(String name) async {
      final file = File('${supportDir.path}/$name');
      final data = await rootBundle.load('assets/bin/$name');
      await file.writeAsBytes(data.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes), flush: true);
      await Process.run('chmod', ['+x', file.path]);
      return Process.start(file.path, []);
    }

    debugPrint('Extracting and starting Android binaries...');
    _goServerProcess = await extractAndRun('json_server');
    _goServerProcess?.stdout.listen((data) => debugPrint('Go: ${String.fromCharCodes(data)}'));
    _goServerProcess?.stderr.listen((data) => debugPrint('Go Error: ${String.fromCharCodes(data)}'));

    _rustThermalProcess = await extractAndRun('thermal_monitor');
    _rustThermalProcess?.stdout.listen((data) => debugPrint('Rust Thermal: ${String.fromCharCodes(data)}'));
    _rustThermalProcess?.stderr.listen((data) => debugPrint('Rust Thermal Error: ${String.fromCharCodes(data)}'));

    _rustClassifierProcess = await extractAndRun('classifier');
    _rustClassifierProcess?.stdout.listen((data) => debugPrint('Rust Classifier: ${String.fromCharCodes(data)}'));
    _rustClassifierProcess?.stderr.listen((data) => debugPrint('Rust Classifier Error: ${String.fromCharCodes(data)}'));
  }

  Future<void> startRuntimes() async {
    try {
      if (Platform.isAndroid) {
        await _extractAndRunAndroid();
        return;
      }

      debugPrint('Starting Go json_server...');
      _goServerProcess = await Process.start(
        'go',
        ['run', 'main/main.go', 'main/json_server.go'],
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
