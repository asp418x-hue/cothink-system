import 'package:cothink_sys/cothink_sys.dart';
import 'package:test/test.dart';

void main() {
  test('runtime options become native environment variables', () {
    const options = CoThinkOptions(
      environment: 'staging',
      configPath: 'config.json',
      verbose: true,
      parameters: <String, String>{'workers': '4'},
    );

    expect(options.toEnvironment(), {
      'COTHINK_ENVIRONMENT': 'staging',
      'COTHINK_CONFIG': 'config.json',
      'COTHINK_VERBOSE': '1',
      'COTHINK_PARAM_WORKERS': '4',
    });
  });
}
