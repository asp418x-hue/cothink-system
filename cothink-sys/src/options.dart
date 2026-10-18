import 'dart:io';

/// Holds the configuration options for the CoThink engine.
class CoThinkOptions {
  /// Target deployment environment (e.g., 'development', 'production').
  final String environment;

  /// Path to external configuration file, if provided.
  final String? configPath;

  /// Whether verbose logging is enabled.
  final bool verbose;

  /// Custom key-value runtime parameters passed to the system.
  final Map<String, dynamic> customParameters;

  const CoThinkOptions({
    this.environment = 'development',
    this.configPath,
    this.verbose = false,
    this.customParameters = const {},
  });

  /// Factory constructor to load options from a file path or environment settings.
  factory CoThinkOptions.fromMap(Map<String, dynamic> map) {
    return CoThinkOptions(
      environment: map['environment'] as String? ?? 'development',
      configPath: map['configPath'] as String?,
      verbose: map['verbose'] as bool? ?? false,
      customParameters: (map['customParameters'] as Map<String, dynamic>?) ?? {},
    );
  }

  /// Converts options back into a map representation.
  Map<String, dynamic> toMap() {
    return {
      'environment': environment,
      'configPath': configPath,
      'verbose': verbose,
      'customParameters': customParameters,
    };
  }

  @override
  String toString() {
    return 'CoThinkOptions(environment: $environment, configPath: $configPath, verbose: $verbose)';
  }
}

