import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:file_picker/file_picker.dart';
import 'config_screen.dart';
import '../widgets/editor_overlay.dart';
import '../services/api_client.dart';
import 'dart:async';

class ToggleEditorIntent extends Intent {
  const ToggleEditorIntent();
}

class CoreDashboard extends StatefulWidget {
  const CoreDashboard({super.key});

  @override
  State<CoreDashboard> createState() => _CoreDashboardState();
}

class _CoreDashboardState extends State<CoreDashboard> {
  bool _showEditor = false;
  Timer? _statusTimer;
  List<dynamic> _agents = [];
  int _activeWorkers = 0;

  @override
  void initState() {
    super.initState();
    _statusTimer = Timer.periodic(const Duration(milliseconds: 500), (_) => _fetchStatus());
  }

  @override
  void dispose() {
    _statusTimer?.cancel();
    super.dispose();
  }

  Future<void> _fetchStatus() async {
    try {
      final status = await ApiClient.getStatus();
      if (mounted && status['status'] != 'error') {
        setState(() {
          _agents = status['agents'] ?? [];
          _activeWorkers = status['active_workers'] ?? 0;
        });
      }
    } catch (e) {
      // Ignore polling errors
    }
  }

  void _toggleEditor() {
    setState(() {
      _showEditor = !_showEditor;
    });
  }

  Future<void> _pickFile() async {
    try {
      FilePickerResult? result = await FilePicker.pickFiles();
      if (result != null) {
        if (!mounted) return;
        
        List<String> paths = result.files.map((e) => e.path).whereType<String>().toList();
        if (paths.isNotEmpty) {
          List<String> contents = [];
          for (String path in paths) {
            try {
              final content = await File(path).readAsString();
              contents.add(content);
            } catch (e) {
              debugPrint('Could not read file: $e');
            }
          }
          
          if (contents.isEmpty) {
            if (!mounted) return;
            ScaffoldMessenger.of(context).showSnackBar(
              const SnackBar(content: Text('No readable text files found')),
            );
            return;
          }

          final response = await ApiClient.digestFileContents(contents);
          if (!mounted) return;
          
          if (response['status'] == 'success') {
            ScaffoldMessenger.of(context).showSnackBar(
              SnackBar(content: Text('Started digesting ${contents.length} file(s)')),
            );
          } else {
            ScaffoldMessenger.of(context).showSnackBar(
              SnackBar(content: Text('Backend error: ${response['error']}')),
            );
          }
        }
      }
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Error loading file: $e')),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    return Shortcuts(
      shortcuts: <LogicalKeySet, Intent>{
        LogicalKeySet(LogicalKeyboardKey.control, LogicalKeyboardKey.keyE): const ToggleEditorIntent(),
      },
      child: Actions(
        actions: <Type, Action<Intent>>{
          ToggleEditorIntent: CallbackAction<ToggleEditorIntent>(
            onInvoke: (ToggleEditorIntent intent) => _toggleEditor(),
          ),
        },
        child: Focus(
          autofocus: true,
          child: Scaffold(
            appBar: AppBar(
              title: const Text('Cothink Orchestrator', style: TextStyle(fontWeight: FontWeight.w600)),
              actions: [
                IconButton(
                  icon: const Icon(Icons.settings),
                  tooltip: 'Configuration',
                  onPressed: () {
                    Navigator.push(
                      context,
                      MaterialPageRoute(builder: (context) => const ConfigScreen()),
                    );
                  },
                ),
                IconButton(
                  icon: const Icon(Icons.code),
                  tooltip: 'Toggle Editor (Ctrl+E)',
                  onPressed: _toggleEditor,
                )
              ],
            ),
            floatingActionButton: Column(
              mainAxisAlignment: MainAxisAlignment.end,
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                FloatingActionButton(
                  heroTag: 'load_file_fab',
                  onPressed: _pickFile,
                  backgroundColor: Theme.of(context).colorScheme.secondary,
                  foregroundColor: Theme.of(context).colorScheme.onSecondary,
                  tooltip: 'Load Task File',
                  child: const Icon(Icons.add),
                ),
                const SizedBox(height: 16),
                FloatingActionButton.extended(
                  heroTag: 'toggle_editor_fab',
                  onPressed: _toggleEditor,
                  icon: Icon(_showEditor ? Icons.close : Icons.terminal),
                  label: Text(_showEditor ? 'Close Editor' : 'Open Editor'),
                  backgroundColor: Theme.of(context).colorScheme.primary,
                  foregroundColor: Theme.of(context).colorScheme.onPrimary,
                ),
              ],
            ),
            body: Stack(
              children: [
                // Main Dashboard Content
                SafeArea(
                  child: ListView(
                    padding: const EdgeInsets.all(24.0),
                    children: [
                      const Text(
                        'Welcome Back,',
                        style: TextStyle(fontSize: 16, color: Colors.grey),
                      ),
                      const SizedBox(height: 8),
                      const Text(
                        'System Overview',
                        style: TextStyle(fontSize: 32, fontWeight: FontWeight.bold, letterSpacing: -0.5),
                      ),
                      const SizedBox(height: 32),
                      Row(
                        children: [
                          Expanded(
                            child: Card(
                              child: InkWell(
                                borderRadius: BorderRadius.circular(16),
                                onTap: () {},
                                child: Padding(
                                  padding: const EdgeInsets.all(24.0),
                                  child: Column(
                                    crossAxisAlignment: CrossAxisAlignment.start,
                                    children: [
                                      Icon(Icons.thermostat, color: Theme.of(context).colorScheme.primary, size: 32),
                                      const SizedBox(height: 16),
                                      const Text('Thermal Monitor', style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600)),
                                      const SizedBox(height: 4),
                                      const Text('System temps & fans', style: TextStyle(color: Colors.grey, fontSize: 12)),
                                    ],
                                  ),
                                ),
                              ),
                            ),
                          ),
                          const SizedBox(width: 16),
                          Expanded(
                            child: Card(
                              child: InkWell(
                                borderRadius: BorderRadius.circular(16),
                                onTap: () {},
                                child: Padding(
                                  padding: const EdgeInsets.all(24.0),
                                  child: Column(
                                    crossAxisAlignment: CrossAxisAlignment.start,
                                    children: [
                                      Icon(Icons.memory, color: Theme.of(context).colorScheme.primary, size: 32),
                                      const SizedBox(height: 16),
                                      const Text('Task Manager', style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600)),
                                      const SizedBox(height: 4),
                                      const Text('Process resources', style: TextStyle(color: Colors.grey, fontSize: 12)),
                                    ],
                                  ),
                                ),
                              ),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 24),
                      Card(
                        child: Padding(
                          padding: const EdgeInsets.all(24.0),
                          child: Row(
                            children: [
                              Icon(Icons.info_outline, color: Theme.of(context).colorScheme.secondary),
                              const SizedBox(width: 16),
                              const Expanded(
                                child: Text(
                                  'Pro Tip: You can also press Ctrl+E on a physical keyboard to instantly toggle the Neovim editor overlay.',
                                  style: TextStyle(height: 1.5),
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                      if (_agents.isNotEmpty) ...[
                        const SizedBox(height: 32),
                        Row(
                          children: [
                            const Text(
                              'Subagent Activity',
                              style: TextStyle(fontSize: 24, fontWeight: FontWeight.bold, letterSpacing: -0.5),
                            ),
                            const Spacer(),
                            if (_activeWorkers > 0)
                              Chip(
                                label: Text('$_activeWorkers Active', style: const TextStyle(fontSize: 12)),
                                backgroundColor: Theme.of(context).colorScheme.primary.withOpacity(0.1),
                                side: BorderSide.none,
                              ),
                          ],
                        ),
                        const SizedBox(height: 16),
                        Wrap(
                          spacing: 12,
                          runSpacing: 12,
                          children: _agents.map((agent) {
                            final int id = agent['id'];
                            final metadata = agent['metadata'] as Map<String, dynamic>? ?? {};
                            final String output = metadata['output'] ?? '';
                            
                            bool isDone = output.isNotEmpty;
                            bool isAnomaly = output.contains('Threshold crossed: true');
                            
                            Color cardColor;
                            if (!isDone) {
                              cardColor = Colors.grey.withOpacity(0.1);
                            } else if (isAnomaly) {
                              cardColor = Colors.red.withOpacity(0.1);
                            } else {
                              cardColor = Colors.green.withOpacity(0.1);
                            }
                            
                            Color iconColor;
                            if (!isDone) {
                              iconColor = Colors.grey;
                            } else if (isAnomaly) {
                              iconColor = Colors.red;
                            } else {
                              iconColor = Colors.green;
                            }

                            return Container(
                              width: 160,
                              padding: const EdgeInsets.all(16),
                              decoration: BoxDecoration(
                                color: cardColor,
                                borderRadius: BorderRadius.circular(16),
                                border: Border.all(
                                  color: isDone ? iconColor.withOpacity(0.3) : Colors.transparent,
                                ),
                              ),
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Row(
                                    children: [
                                      Icon(isDone ? (isAnomaly ? Icons.warning : Icons.check_circle) : Icons.pending, color: iconColor, size: 18),
                                      const SizedBox(width: 8),
                                      Text('Agent $id', style: const TextStyle(fontWeight: FontWeight.bold)),
                                    ],
                                  ),
                                  const SizedBox(height: 12),
                                  Text(
                                    isDone ? (isAnomaly ? 'Anomaly Detected' : 'Normal') : 'Analyzing...',
                                    style: TextStyle(fontSize: 12, color: isDone ? iconColor : Colors.grey, fontWeight: FontWeight.w600),
                                  ),
                                ],
                              ),
                            );
                          }).toList(),
                        ),
                      ],
                    ],
                  ),
                ),
                
                // Editor Overlay
                if (_showEditor)
                  Positioned.fill(
                    child: AnimatedOpacity(
                      opacity: _showEditor ? 1.0 : 0.0,
                      duration: const Duration(milliseconds: 200),
                      child: SafeArea(
                        child: Padding(
                          padding: const EdgeInsets.all(16.0),
                          child: Material(
                            elevation: 24,
                            borderRadius: BorderRadius.circular(16.0),
                            clipBehavior: Clip.antiAlias,
                            child: NeovimEditorOverlay(
                              onClose: () {
                                setState(() {
                                  _showEditor = false;
                                });
                              },
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
