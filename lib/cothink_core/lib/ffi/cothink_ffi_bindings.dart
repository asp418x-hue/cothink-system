import 'dart:ffi';
import 'dart:io';
import 'package:ffi/ffi.dart';

/// Low-level C-ABI Struct for Field of View (FOV) telemetry
final class NativeFov extends Struct {
  @IntPtr()
  external int activeWorkers;

  @Double()
  external double tempCelsius;

  @Uint64()
  external int cpuFreqMhz;

  @IntPtr()
  external int recentSuccess;

  @IntPtr()
  external int recentFailure;
}

/// Low-level C-ABI Struct for Thermal Zone sample
final class NativeThermalSample extends Struct {
  @Uint32()
  external int zoneId;

  @Double()
  external double tempCelsius;

  @Uint64()
  external int cpuFreqMhz;

  @Bool()
  external bool isThrottling;
}

/// Low-level C-ABI Struct for Anomaly Classification
final class NativeAnomalyResult extends Struct {
  @Uint64()
  external int frameId;

  @Double()
  external double score;

  @Bool()
  external bool thresholdCrossed;
}

// Native function pointer typedefs
typedef _CothinkInitC = Int32 Function();
typedef _CothinkInitDart = int Function();

typedef _CothinkGetFovC = Int32 Function(Uint64 orchId, Pointer<NativeFov> outFov);
typedef _CothinkGetFovDart = int Function(int orchId, Pointer<NativeFov> outFov);

typedef _CothinkRecordEventC = Int32 Function(
  Uint64 orchId,
  IntPtr subagentId,
  Pointer<Utf8> event,
  Bool success,
  Pointer<Utf8> detail,
);
typedef _CothinkRecordEventDart = int Function(
  int orchId,
  int subagentId,
  Pointer<Utf8> event,
  bool success,
  Pointer<Utf8> detail,
);

typedef _CothinkThermalSampleC = Int32 Function(
  Uint32 zoneId,
  Double simulatedTemp,
  Double criticalTemp,
  Pointer<NativeThermalSample> outSample,
);
typedef _CothinkThermalSampleDart = int Function(
  int zoneId,
  double simulatedTemp,
  double criticalTemp,
  Pointer<NativeThermalSample> outSample,
);

typedef _CothinkClassifyC = Int32 Function(
  Uint64 frameId,
  Pointer<NativeAnomalyResult> outResult,
);
typedef _CothinkClassifyDart = int Function(
  int frameId,
  Pointer<NativeAnomalyResult> outResult,
);

typedef _CothinkExecutePayloadC = Pointer<Utf8> Function(Pointer<Utf8> payloadJson);
typedef _CothinkExecutePayloadDart = Pointer<Utf8> Function(Pointer<Utf8> payloadJson);

typedef _CothinkFreeStringC = Void Function(Pointer<Utf8> ptr);
typedef _CothinkFreeStringDart = void Function(Pointer<Utf8> ptr);

/// Low-level FFI bindings to libcothink_system
class CothinkFfiBindings {
  final DynamicLibrary _lib;

  late final _CothinkInitDart cothinkInit;
  late final _CothinkGetFovDart cothinkGetFov;
  late final _CothinkRecordEventDart cothinkRecordEvent;
  late final _CothinkThermalSampleDart cothinkThermalSample;
  late final _CothinkClassifyDart cothinkClassify;
  late final _CothinkExecutePayloadDart cothinkExecutePayload;
  late final _CothinkFreeStringDart cothinkFreeString;

  CothinkFfiBindings(this._lib) {
    cothinkInit = _lib.lookupFunction<_CothinkInitC, _CothinkInitDart>('cothink_init');
    cothinkGetFov = _lib.lookupFunction<_CothinkGetFovC, _CothinkGetFovDart>('cothink_get_fov');
    cothinkRecordEvent = _lib.lookupFunction<_CothinkRecordEventC, _CothinkRecordEventDart>('cothink_record_event');
    cothinkThermalSample = _lib.lookupFunction<_CothinkThermalSampleC, _CothinkThermalSampleDart>('cothink_thermal_sample');
    cothinkClassify = _lib.lookupFunction<_CothinkClassifyC, _CothinkClassifyDart>('cothink_classify');
    cothinkExecutePayload = _lib.lookupFunction<_CothinkExecutePayloadC, _CothinkExecutePayloadDart>('cothink_execute_payload');
    cothinkFreeString = _lib.lookupFunction<_CothinkFreeStringC, _CothinkFreeStringDart>('cothink_free_string');
  }

  /// Locates and opens libcothink_system dynamic library across supported platforms
  static DynamicLibrary openLibrary() {
    if (Platform.isAndroid) {
      return DynamicLibrary.open('libcothink_system.so');
    }

    if (Platform.isLinux) {
      final candidates = [
        'libcothink_system.so',
        'target_local/debug/libcothink_system.so',
        'target_local/release/libcothink_system.so',
        'target/debug/libcothink_system.so',
        'target/release/libcothink_system.so',
        '/usr/local/lib/libcothink_system.so',
        '/usr/lib/libcothink_system.so',
      ];

      for (final candidate in candidates) {
        try {
          return DynamicLibrary.open(candidate);
        } catch (_) {}
      }
    }

    if (Platform.isMacOS) {
      final candidates = [
        'libcothink_system.dylib',
        'target_local/debug/libcothink_system.dylib',
        'target/debug/libcothink_system.dylib',
      ];
      for (final candidate in candidates) {
        try {
          return DynamicLibrary.open(candidate);
        } catch (_) {}
      }
    }

    if (Platform.isWindows) {
      final candidates = [
        'cothink_system.dll',
        'target_local/debug/cothink_system.dll',
        'target/debug/cothink_system.dll',
      ];
      for (final candidate in candidates) {
        try {
          return DynamicLibrary.open(candidate);
        } catch (_) {}
      }
    }

    // Default attempt
    return DynamicLibrary.process();
  }
}
