import 'dart:convert';
import 'dart:io';

class ApiClient {
  static const int port = 5000;
  static const String host = '127.0.0.1';

  static Future<Map<String, dynamic>> _sendRequest(Map<String, dynamic> requestData) async {
    Socket? socket;
    try {
      socket = await Socket.connect(host, port, timeout: const Duration(seconds: 2));
      
      final payload = jsonEncode(requestData);
      socket.write(payload);
      
      final responseBuffer = StringBuffer();
      await for (final data in socket.cast<List<int>>().transform(utf8.decoder)) {
        responseBuffer.write(data);
      }
      
      if (responseBuffer.isNotEmpty) {
        return jsonDecode(responseBuffer.toString()) as Map<String, dynamic>;
      }
      return {'status': 'error', 'error': 'Empty response'};
    } catch (e) {
      return {'status': 'error', 'error': e.toString()};
    } finally {
      socket?.destroy();
    }
  }

  static Future<Map<String, dynamic>> getStatus() async {
    return _sendRequest({'action': 'get_status'});
  }

  static Future<Map<String, dynamic>> setConcurrency(int limit) async {
    return _sendRequest({
      'action': 'set_concurrency',
      'limit': limit,
    });
  }

  static Future<Map<String, dynamic>> zeroResiduals() async {
    return _sendRequest({'action': 'zero_residuals'});
  }

  static Future<Map<String, dynamic>> triggerSweep() async {
    return _sendRequest({'action': 'trigger_sweep'});
  }

  static Future<Map<String, dynamic>> digestFiles(List<String> paths) async {
    return _sendRequest({
      'action': 'digest_files',
      'paths': paths,
    });
  }

  static Future<Map<String, dynamic>> digestFileContents(List<String> contents) async {
    return _sendRequest({
      'action': 'digest_files_content',
      'contents': contents,
    });
  }

  static Future<Map<String, dynamic>> executeInstruction(Map<String, dynamic> instruction) async {
    return _sendRequest({
      'action': 'execute_instruction',
      'instruction': instruction,
    });
  }
}
