import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:file_picker/file_picker.dart';
import 'config_screen.dart';
import '../widgets/editor_overlay.dart';

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

  void _toggleEditor() {
    setState(() {
      _showEditor = !_showEditor;
    });
  }

  Future<void> _pickFile() async {
    try {
      FilePickerResult? result = await FilePicker.platform.pickFiles();
      if (result != null) {
        if (!mounted) return;
        // Placeholder for file digestion logic
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Loaded: ${result.files.single.name} for digestion')),
        );
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
