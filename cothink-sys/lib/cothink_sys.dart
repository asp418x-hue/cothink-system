/// Runtime configuration passed from the Dart CLI to the native engine.
class CoThinkOptions {
  const CoThinkOptions({
    this.environment = 'development',
    this.configPath,
    this.verbose = false,
    this.parameters = const <String, String>{},
  });

  final String environment;
  final String? configPath;
  final bool verbose;
  final Map<String, String> parameters;

  Map<String, String> toEnvironment() => <String, String>{
    'COTHINK_ENVIRONMENT': environment,
    if (configPath != null) 'COTHINK_CONFIG': configPath!,
    if (verbose) 'COTHINK_VERBOSE': '1',
    for (final entry in parameters.entries)
      'COTHINK_PARAM_${entry.key.toUpperCase()}': entry.value,
  };
}
