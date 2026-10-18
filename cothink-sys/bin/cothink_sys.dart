import 'dart:async';
import 'dart:io';

import 'package:args/args.dart';
import 'package:cothink_sys/cothink_sys.dart';

// Import your library package exports if available
// import 'package:cothink_sys/cothink_sys.dart';

Future<void> main(List<String> arguments) async {
  final parser = ArgParser()
    ..addFlag(
      'help',
      abbr: 'h',
      negatable: false,
      help: 'Print usage information for cothink_sys.',
    )
    ..addFlag(
      'version',
      abbr: 'v',
      negatable: false,
      help: 'Print current package version.',
    )
    ..addOption(
      'config',
      abbr: 'c',
      help: 'Path to system configuration YAML/JSON file.',
      valueHelp: 'path/to/config.yaml',
    )
    ..addOption(
      'environment',
      abbr: 'e',
      defaultsTo: 'development',
      allowed: ['development', 'staging', 'production'],
      help: 'Target deployment/execution environment.',
    )
    ..addFlag(
      'verbose',
      abbr: 'V',
      negatable: false,
      help: 'Enable detailed debug logging output.',
    )
    ..addOption(
      'binary',
      help: 'Native cothink-system executable (defaults to PATH or COTHINK_SYSTEM_BIN).',
      valueHelp: 'path/to/cothink-system',
    );

  try {
    final ArgResults argResults = parser.parse(arguments);

    // 1. Handle top-level help
    if (argResults['help'] as bool) {
      _printHelp(parser);
      exit(0);
    }

    // 2. Handle version check
    if (argResults['version'] as bool) {
      print('cothink_sys version 0.1.0');
      exit(0);
    }

    // Extract runtime settings and launch the native orchestrator.
    final isVerbose = argResults['verbose'] as bool;
    final configPath = argResults['config'] as String?;
    final environment = argResults['environment'] as String;
    final binary =
        argResults['binary'] as String? ??
        Platform.environment['COTHINK_SYSTEM_BIN'] ??
        'cothink-system';

    if (isVerbose) {
      stderr.writeln('[cothink_sys] launching $binary');
    }

    final options = CoThinkOptions(
      environment: environment,
      configPath: configPath,
      verbose: isVerbose,
    );
    final code = await _runCothinkSystem(binary, options, argResults.rest);
    exit(code);
  } on FormatException catch (e) {
    stderr.writeln('Error: ${e.message}\n');
    _printHelp(parser);
    exit(64); // Exit code 64: EX_USAGE (command line usage error)
  } catch (e, stackTrace) {
    stderr.writeln('Unhandled exception: $e');
    if (arguments.contains('-V') || arguments.contains('--verbose')) {
      stderr.writeln(stackTrace);
    }
    exit(1);
  }
}

Future<int> _runCothinkSystem(
  String binary,
  CoThinkOptions options,
  List<String> restArgs,
) async {
  try {
    final process = await Process.start(
      binary,
      restArgs,
      environment: <String, String>{
        ...Platform.environment,
        ...options.toEnvironment(),
      },
      mode: ProcessStartMode.inheritStdio,
    );
    return await process.exitCode;
  } on ProcessException catch (error) {
    stderr.writeln('Unable to start "$binary": ${error.message}');
    stderr.writeln('Install cothink-system or pass --binary <path>.');
    return 127;
  }
}

void _printHelp(ArgParser parser) {
  print('''
cothink_sys - Command Line Interface for cothink-system

Usage:
  dart run bin/cothink_sys.dart [options] [-- native-arguments]

Options:''');
  print(parser.usage);
}
