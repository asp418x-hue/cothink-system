import 'package:flutter/material.dart';
import '../services/api_client.dart';
import 'dart:io';
import 'dart:convert';
import 'package:path_provider/path_provider.dart';

class InstructionBuilderScreen extends StatefulWidget {
  const InstructionBuilderScreen({super.key});

  @override
  State<InstructionBuilderScreen> createState() => _InstructionBuilderScreenState();
}

class _InstructionBuilderScreenState extends State<InstructionBuilderScreen> {
  final TextEditingController _agentsController = TextEditingController(text: '10');
  final TextEditingController _delayController = TextEditingController(text: '100');
  final TextEditingController _payloadController = TextEditingController(text: '42,0.95');

  bool _isExecuting = false;

  @override
  void dispose() {
    _agentsController.dispose();
    _delayController.dispose();
    _payloadController.dispose();
    super.dispose();
  }

  Future<void> _executeInstruction() async {
    setState(() {
      _isExecuting = true;
    });

    try {
      final maxChildren = int.tryParse(_agentsController.text) ?? 5;
      final baseDelay = int.tryParse(_delayController.text) ?? 150;
      final payload = _payloadController.text;

      // 1. Send to Go backend to spawn the swarm
      final instruction = {
        'max_children': maxChildren,
        'base_delay_ms': baseDelay,
        'payload': payload,
      };
      
      // We don't await this so it happens in parallel
      ApiClient.executeInstruction(instruction).catchError((e) {
        debugPrint('Go backend instruction failed: $e');
        return <String, dynamic>{};
      });

      // 2. Execute natively in Rust for immediate host telemetry
      String execPath = '/home/asp418x/cothink-system/bin/payload_executor';
      if (Platform.isAndroid) {
        final supportDir = await getApplicationSupportDirectory();
        execPath = '${supportDir.path}/payload_executor';
      }

      final process = await Process.start(execPath, []);
      process.stdin.write(payload);
      await process.stdin.flush();
      await process.stdin.close();

      final outputBytes = await process.stdout.toList();
      final output = utf8.decode(outputBytes.expand((x) => x).toList());

      if (!mounted) return;
      
      try {
        final Map<String, dynamic> response = jsonDecode(output);
        if (response['status'] == 'success') {
          String msg = 'Native Rust Execution: Score ${response['score']}';
          if (response['threshold_exceeded'] == true) {
             msg += '\n\n[DIAGNOSTIC]: ${response['diagnostic']}';
             if (response.containsKey('solution')) {
               msg += '\n\n[SUGGESTED SOLUTION]:\n${response['solution']}';
             }
          }
          
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(
              content: Text(msg, style: const TextStyle(height: 1.4)),
              duration: const Duration(seconds: 15),
              backgroundColor: response['threshold_exceeded'] == true ? Colors.red.shade900 : Colors.green.shade900,
            ),
          );
          Navigator.pop(context); // Go back to dashboard
        } else {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text('Native Error: ${response['error']}')),
          );
        }
      } catch (e) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Failed to parse Rust output: $e\nOutput: $output')),
        );
      }
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Failed to execute instruction: $e')),
      );
    } finally {
      if (mounted) {
        setState(() {
          _isExecuting = false;
        });
      }
    }
  }

  Widget _buildInlineInput(TextEditingController controller, double width, String hint) {
    return Container(
      width: width,
      margin: const EdgeInsets.symmetric(horizontal: 8.0),
      child: TextField(
        controller: controller,
        textAlign: TextAlign.center,
        style: const TextStyle(
          color: Colors.cyanAccent,
          fontWeight: FontWeight.bold,
          fontSize: 24,
        ),
        decoration: InputDecoration(
          hintText: hint,
          hintStyle: const TextStyle(color: Colors.white30),
          filled: true,
          fillColor: Colors.white10,
          contentPadding: const EdgeInsets.symmetric(vertical: 4, horizontal: 8),
          border: OutlineInputBorder(
            borderRadius: BorderRadius.circular(8),
            borderSide: BorderSide.none,
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Create Instruction'),
      ),
      body: Center(
        child: Padding(
          padding: const EdgeInsets.all(32.0),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            crossAxisAlignment: CrossAxisAlignment.center,
            children: [
              Wrap(
                alignment: WrapAlignment.center,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  const Text(
                    'Deploy ',
                    style: TextStyle(fontSize: 24, fontWeight: FontWeight.w500),
                  ),
                  _buildInlineInput(_agentsController, 80, '10'),
                  const Text(
                    ' anomaly detection agents',
                    style: TextStyle(fontSize: 24, fontWeight: FontWeight.w500),
                  ),
                ],
              ),
              const SizedBox(height: 24),
              Wrap(
                alignment: WrapAlignment.center,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  const Text(
                    'with a base initialization delay of ',
                    style: TextStyle(fontSize: 24, fontWeight: FontWeight.w500),
                  ),
                  _buildInlineInput(_delayController, 100, '100'),
                  const Text(
                    ' milliseconds',
                    style: TextStyle(fontSize: 24, fontWeight: FontWeight.w500),
                  ),
                ],
              ),
              const SizedBox(height: 24),
              Wrap(
                alignment: WrapAlignment.center,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  const Text(
                    'to process the telemetry payload:',
                    style: TextStyle(fontSize: 24, fontWeight: FontWeight.w500),
                  ),
                  _buildInlineInput(_payloadController, 200, '42,0.95'),
                  const Text(
                    '.',
                    style: TextStyle(fontSize: 24, fontWeight: FontWeight.w500),
                  ),
                ],
              ),
              const SizedBox(height: 64),
              ElevatedButton.icon(
                onPressed: _isExecuting ? null : _executeInstruction,
                icon: _isExecuting 
                    ? const SizedBox(width: 24, height: 24, child: CircularProgressIndicator(color: Colors.white, strokeWidth: 2)) 
                    : const Icon(Icons.rocket_launch, size: 28),
                label: const Text(
                  'EXECUTE INSTRUCTION',
                  style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold, letterSpacing: 1.5),
                ),
                style: ElevatedButton.styleFrom(
                  padding: const EdgeInsets.symmetric(horizontal: 48, vertical: 24),
                  backgroundColor: Theme.of(context).colorScheme.primary,
                  foregroundColor: Theme.of(context).colorScheme.onPrimary,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(32),
                  ),
                  elevation: 12,
                  shadowColor: Theme.of(context).colorScheme.primary.withOpacity(0.5),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
