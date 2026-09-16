import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'package:ffi/ffi.dart';
import 'package:flutter/foundation.dart';
import 'cothink_ffi_bindings.dart';

/// Dart model for Field of View (FOV) telemetry
class FovData {
  final int activeWorkers;
  final double tempCelsius;
  final int cpuFreqMhz;
  final int recentSuccess;
  final int recentFailure;

  const FovData({
    required this.activeWorkers,
    required this.tempCelsius,
    required this.cpuFreqMhz,
    required this.recentSuccess,
    required this.recentFailure,
  });

  @override
  String toString() =>
      'FovData(workers: $activeWorkers, temp: ${tempCelsius.toStringAsFixed(1)}°C, freq: ${cpuFreqMhz}MHz, success: $recentSuccess, fail: $recentFailure)';
}

/// Dart model for Thermal Zone telemetry
class ThermalSample {
  final int zoneId;
  final double tempCelsius;
  final int cpuFreqMhz;
  final bool isThrottling;

  const ThermalSample({
    required this.zoneId,
    required this.tempCelsius,
    required this.cpuFreqMhz,
    required this.isThrottling,
  });

  @override
  String toString() =>
      'ThermalSample(zone: $zoneId, temp: ${tempCelsius.toStringAsFixed(1)}°C, freq: ${cpuFreqMhz}MHz, throttling: $isThrottling)';
}

/// Dart model for Anomaly Classification
class AnomalyResult {
  final int frameId;
  final double score;
  final bool thresholdCrossed;

  const AnomalyResult({
    required this.frameId,
    required this.score,
    required this.thresholdCrossed,
  });

  @override
  String toString() =>
      'AnomalyResult(frame: $frameId, score: ${score.toStringAsFixed(2)}, crossed: $thresholdCrossed)';
}

/// High-level, thread-safe Dart FFI client for Cothink System
class CothinkNative {
  static final CothinkNative _instance = CothinkNative._internal();
  factory CothinkNative() => _instance;

  CothinkFfiBindings? _bindings;
  bool _initialized = false;

  CothinkNative._internal() {
    _initLibrary();
  }

  void _initLibrary() {
    try {
      final lib = CothinkFfiBindings.openLibrary();
      _bindings = CothinkFfiBindings(lib);
      final rc = _bindings!.cothinkInit();
      _initialized = (rc == 0);
      debugPrint('[CothinkNative] Loaded native library successfully (init rc=$rc).');
    } catch (e) {
      debugPrint('[CothinkNative] Could not load native library: $e');
      _bindings = null;
      _initialized = false;
    }
  }

  /// Whether the native library is loaded and operational
  bool get isAvailable => _initialized && _bindings != null;

  /// Retrieve current Field of View telemetry
  FovData? getFov({int orchId = 1}) {
    if (!isAvailable) return null;

    final outPtr = calloc<NativeFov>();
    try {
      final rc = _bindings!.cothinkGetFov(orchId, outPtr);
      if (rc != 0) return null;

      final native = outPtr.ref;
      return FovData(
        activeWorkers: native.activeWorkers,
        tempCelsius: native.tempCelsius,
        cpuFreqMhz: native.cpuFreqMhz,
        recentSuccess: native.recentSuccess,
        recentFailure: native.recentFailure,
      );
    } finally {
      calloc.free(outPtr);
    }
  }

  /// Sample a thermal zone and CPU governor frequency
  ThermalSample? getThermalSample({
    int zoneId = 0,
    double simulatedTemp = 45.0,
    double criticalTemp = 75.0,
  }) {
    if (!isAvailable) return null;

    final outPtr = calloc<NativeThermalSample>();
    try {
      final rc = _bindings!.cothinkThermalSample(zoneId, simulatedTemp, criticalTemp, outPtr);
      if (rc != 0) return null;

      final native = outPtr.ref;
      return ThermalSample(
        zoneId: native.zoneId,
        tempCelsius: native.tempCelsius,
        cpuFreqMhz: native.cpuFreqMhz,
        isThrottling: native.isThrottling,
      );
    } finally {
      calloc.free(outPtr);
    }
  }

  /// Classify a sensor frame by ID
  AnomalyResult? classify(int frameId) {
    if (!isAvailable) return null;

    final outPtr = calloc<NativeAnomalyResult>();
    try {
      final rc = _bindings!.cothinkClassify(frameId, outPtr);
      if (rc != 0) return null;

      final native = outPtr.ref;
      return AnomalyResult(
        frameId: native.frameId,
        score: native.score,
        thresholdCrossed: native.thresholdCrossed,
      );
    } finally {
      calloc.free(outPtr);
    }
  }

  /// Record an event into native history buffer
  bool recordEvent({
    int orchId = 1,
    int subagentId = 0,
    required String event,
    bool success = true,
    String detail = '',
  }) {
    if (!isAvailable) return false;

    final eventPtr = event.toNativeUtf8();
    final detailPtr = detail.toNativeUtf8();
    try {
      final rc = _bindings!.cothinkRecordEvent(orchId, subagentId, eventPtr, success, detailPtr);
      return rc == 0;
    } finally {
      calloc.free(eventPtr);
      calloc.free(detailPtr);
    }
  }

  /// Execute payload metrics analysis and return parsed JSON
  Map<String, dynamic>? executePayload(String payloadJson) {
    if (!isAvailable) return null;

    final inputPtr = payloadJson.toNativeUtf8();
    try {
      final resultPtr = _bindings!.cothinkExecutePayload(inputPtr);
      if (resultPtr == nullptr) return null;

      try {
        final jsonString = resultPtr.toDartString();
        return jsonDecode(jsonString) as Map<String, dynamic>;
      } finally {
        _bindings!.cothinkFreeString(resultPtr);
      }
    } catch (e) {
      debugPrint('[CothinkNative] executePayload error: $e');
      return null;
    } finally {
      calloc.free(inputPtr);
    }
  }

  /// Periodic stream of thermal samples directly from native runtime
  Stream<ThermalSample> streamThermal({
    int zoneId = 0,
    Duration interval = const Duration(seconds: 1),
  }) {
    late StreamController<ThermalSample> controller;
    Timer? timer;
    double stepTemp = 42.0;

    void tick() {
      stepTemp = (stepTemp >= 82.0) ? 45.0 : stepTemp + 3.5;
      final sample = getThermalSample(zoneId: zoneId, simulatedTemp: stepTemp);
      if (sample != null && !controller.isClosed) {
        controller.add(sample);
      }
    }

    controller = StreamController<ThermalSample>(
      onListen: () {
        tick();
        timer = Timer.periodic(interval, (_) => tick());
      },
      onCancel: () {
        timer?.cancel();
      },
    );

    return controller.stream;
  }
}
