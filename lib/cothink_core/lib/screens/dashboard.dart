import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import '../widgets/editor_overlay.dart';

class ToggleEditorIntent extends Intent {
  const ToggleEditorIntent();
}

class CoreDashboard extends StatefulWidget {
  const CoreDashboard({Key? key}) : super(key: key);

  @override
  _CoreDashboardState createState() => _CoreDashboardState();
}

class _CoreDashboardState extends State<CoreDashboard> {
  bool _showEditor = false;

  void _toggleEditor() {
    setState(() {
      _showEditor = !_showEditor;
    });
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
              title: const Text('Cothink System Dashboard'),
              actions: [
                IconButton(
                  icon: const Icon(Icons.code),
                  tooltip: 'Toggle Editor (Ctrl+E)',
                  onPressed: _toggleEditor,
                )
              ],
            ),
            body: Stack(
              children: [
                // Main Dashboard Content
                Center(
                  child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      const Text(
                        'Welcome to the Cothink Orchestrator',
                        style: TextStyle(fontSize: 24, fontWeight: FontWeight.bold),
                      ),
                      const SizedBox(height: 20),
                      ElevatedButton(
                        onPressed: () {
                          // Placeholder for future actions
                        },
                        child: const Text('View Thermal Monitor'),
                      ),
                      const SizedBox(height: 10),
                      const Text('Press Ctrl+E to open the Neovim editor.'),
                    ],
                  ),
                ),
                
                // Editor Overlay
                if (_showEditor)
                  Positioned.fill(
                    child: Padding(
                      padding: const EdgeInsets.all(32.0),
                      child: Material(
                        elevation: 10,
                        borderRadius: BorderRadius.circular(8.0),
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
              ],
            ),
          ),
        ),
      ),
    );
  }
}
