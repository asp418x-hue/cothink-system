import 'dart:io';
import 'dart:async';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'path_utils.dart';
class RuntimeManager {
  Process? _goServerProcess;
  Process? _rustThermalProcess;
  Process? _rustClassifierProcess;
  
  final StreamController<String> thermalStream = StreamController<String>.broadcast();
  final StreamController<String> classifierStream = StreamController<String>.broadcast();
  
  static final RuntimeManager _instance = RuntimeManager._internal();
  factory RuntimeManager() => _instance;
  RuntimeManager._internal();

  Future<void> _extractAndRunAndroid() async {
    final supportDir = await getApplicationSupportDirectory();
    final rootfsDir = Directory('${supportDir.path}/rootfs');

    // Extract proot binary
    final prootFile = File('${supportDir.path}/proot');
    try {
      final prootData = await rootBundle.load('assets/bin/proot');
      await prootFile.writeAsBytes(prootData.buffer.asUint8List(prootData.offsetInBytes, prootData.lengthInBytes), flush: true);
      await Process.run('chmod', ['+x', prootFile.path]);
    } catch (e) {
      debugPrint('Warning: Could not extract proot binary from assets: $e');
    }

    // Extract payload_executor binary for native Dart execution
    final payloadExecutorFile = File('${supportDir.path}/payload_executor');
    try {
      final executorData = await rootBundle.load('assets/bin/payload_executor');
      await payloadExecutorFile.writeAsBytes(executorData.buffer.asUint8List(executorData.offsetInBytes, executorData.lengthInBytes), flush: true);
      await Process.run('chmod', ['+x', payloadExecutorFile.path]);
    } catch (e) {
      debugPrint('Warning: Could not extract payload_executor binary from assets: $e');
    }

    if (!await rootfsDir.exists()) {
      debugPrint('Rootfs not found, extracting from assets...');
      await rootfsDir.create(recursive: true);
      final tarballFile = File('${supportDir.path}/antiX-rootfs.tar.gz');
      
      try {
        final tarballData = await rootBundle.load('assets/rootfs/antiX-rootfs.tar.gz');
        if (tarballData.lengthInBytes > 0) {
          await tarballFile.writeAsBytes(tarballData.buffer.asUint8List(tarballData.offsetInBytes, tarballData.lengthInBytes), flush: true);
          
          // Extract tarball using native tar
          debugPrint('Decompressing antiX rootfs...');
          final tarResult = await Process.run('tar', ['-xzf', tarballFile.path, '-C', rootfsDir.path]);
          if (tarResult.exitCode != 0) {
            debugPrint('Error extracting tarball: ${tarResult.stderr}');
          }
          await tarballFile.delete(); // Cleanup
        }
      } catch (e) {
        debugPrint('Warning: Could not extract antiX-rootfs.tar.gz from assets: $e');
      }
    }

    // Start the Go server using proot
    debugPrint('Starting proot container...');
    try {
      _goServerProcess = await Process.start(
        prootFile.path,
        [
          '-r', rootfsDir.path,
          '-0', // fake root
          '-b', '/dev',
          '-b', '/proc',
          '-b', '/sys',
          '-w', '/opt/cothink',
          '/bin/bash', '-c', './json_server'
        ],
      );
      _goServerProcess?.stdout.listen((data) => debugPrint('Proot: ${String.fromCharCodes(data)}'));
      _goServerProcess?.stderr.listen((data) => debugPrint('Proot Error: ${String.fromCharCodes(data)}'));

      debugPrint('Starting Rust thermal_monitor in proot...');
      _rustThermalProcess = await Process.start(
        prootFile.path,
        [
          '-r', rootfsDir.path,
          '-0',
          '-b', '/dev',
          '-b', '/proc',
          '-b', '/sys',
          '-w', '/opt/cothink',
          '/bin/bash', '-c', './thermal_monitor'
        ],
      );
      _rustThermalProcess?.stdout.listen((data) {
        final str = String.fromCharCodes(data);
        debugPrint('Proot Thermal: $str');
        thermalStream.add(str);
      });
      _rustThermalProcess?.stderr.listen((data) => debugPrint('Proot Thermal Error: ${String.fromCharCodes(data)}'));

      debugPrint('Starting Rust classifier in proot...');
      _rustClassifierProcess = await Process.start(
        prootFile.path,
        [
          '-r', rootfsDir.path,
          '-0',
          '-b', '/dev',
          '-b', '/proc',
          '-b', '/sys',
          '-w', '/opt/cothink',
          '/bin/bash', '-c', './classifier'
        ],
      );
      _rustClassifierProcess?.stdout.listen((data) {
        final str = String.fromCharCodes(data);
        debugPrint('Proot Classifier: $str');
        classifierStream.add(str);
      });
      _rustClassifierProcess?.stderr.listen((data) => debugPrint('Proot Classifier Error: ${String.fromCharCodes(data)}'));

    } catch (e) {
      debugPrint('Error starting proot container: $e');
    }
  }

  Future<void> startRuntimes() async {
    try {
      if (Platform.isAndroid) {
        await _extractAndRunAndroid();
        return;
      }

      if (_goServerProcess != null) {
        // Already started
        return;
      }

      debugPrint('Starting Go json_server...');
      _goServerProcess = await Process.start(
        PathUtils.binaryPath('json_server'),
        [],
        runInShell: false,
        workingDirectory: PathUtils.projectRoot,
      );
      _goServerProcess?.stdout.listen((data) => debugPrint('Go: ${String.fromCharCodes(data)}'));
      _goServerProcess?.stderr.listen((data) => debugPrint('Go Error: ${String.fromCharCodes(data)}'));

      debugPrint('Starting Rust thermal_monitor...');
      _rustThermalProcess = await Process.start(
        PathUtils.binaryPath('thermal_monitor'),
        [],
        runInShell: false,
        workingDirectory: PathUtils.projectRoot,
      );
      _rustThermalProcess?.stdout.listen((data) {
        final str = String.fromCharCodes(data);
        debugPrint('Rust Thermal: $str');
        thermalStream.add(str);
      });
      _rustThermalProcess?.stderr.listen((data) => debugPrint('Rust Thermal Error: ${String.fromCharCodes(data)}'));

      debugPrint('Starting Rust classifier...');
      _rustClassifierProcess = await Process.start(
        PathUtils.binaryPath('classifier'),
        [],
        runInShell: false,
        workingDirectory: PathUtils.projectRoot,
      );
      _rustClassifierProcess?.stdout.listen((data) {
        final str = String.fromCharCodes(data);
        debugPrint('Rust Classifier: $str');
        classifierStream.add(str);
      });
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
    _goServerProcess = null;
    _rustThermalProcess = null;
    _rustClassifierProcess = null;
  }
}
