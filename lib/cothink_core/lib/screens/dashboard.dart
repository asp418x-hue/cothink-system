import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'config_screen.dart';
import 'instruction_builder.dart';
import '../widgets/editor_overlay.dart';
import '../widgets/dashboard/thermal_card.dart';
import '../widgets/dashboard/process_card.dart';
import '../widgets/dashboard/agent_activity_section.dart';
import '../services/dashboard_controller.dart';

class ToggleEditorIntent extends Intent {
  const ToggleEditorIntent();
}

class CoreDashboard extends StatefulWidget {
  const CoreDashboard({super.key});

  @override
  State<CoreDashboard> createState() => _CoreDashboardState();
}

class _CoreDashboardState extends State<CoreDashboard> {
  final DashboardController _controller = DashboardController();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
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
            onInvoke: (ToggleEditorIntent intent) => _controller.toggleEditor(),
          ),
        },
        child: Focus(
          autofocus: true,
          child: AnimatedBuilder(
            animation: _controller,
            builder: (context, child) {
              return Scaffold(
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
                      onPressed: _controller.toggleEditor,
                    )
                  ],
                ),
                floatingActionButton: Column(
                  mainAxisAlignment: MainAxisAlignment.end,
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    FloatingActionButton.extended(
                      heroTag: 'toggle_editor_fab',
                      onPressed: _controller.toggleEditor,
                      icon: Icon(_controller.showEditor ? Icons.close : Icons.terminal),
                      label: Text(_controller.showEditor ? 'Close Editor' : 'Open Editor'),
                      backgroundColor: Theme.of(context).colorScheme.primary,
                      foregroundColor: Theme.of(context).colorScheme.onPrimary,
                    ),
                    const SizedBox(height: 16),
                    FloatingActionButton.extended(
                      heroTag: 'instruction_builder_fab',
                      onPressed: () {
                        Navigator.push(
                          context,
                          MaterialPageRoute(builder: (context) => const InstructionBuilderScreen()),
                        );
                      },
                      icon: const Icon(Icons.auto_fix_high),
                      label: const Text('Create Instruction'),
                      backgroundColor: Colors.deepPurpleAccent,
                      foregroundColor: Colors.white,
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
                            children: const [
                              Expanded(child: ThermalMonitorCard()),
                              SizedBox(width: 16),
                              Expanded(child: ProcessMonitorCard()),
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
                          AgentActivitySection(
                            agents: _controller.agents,
                            activeWorkers: _controller.activeWorkers,
                          ),
                        ],
                      ),
                    ),
                    
                    // Editor Overlay
                    if (_controller.showEditor)
                      Positioned.fill(
                        child: AnimatedOpacity(
                          opacity: _controller.showEditor ? 1.0 : 0.0,
                          duration: const Duration(milliseconds: 200),
                          child: SafeArea(
                            child: Padding(
                              padding: const EdgeInsets.all(16.0),
                              child: Material(
                                elevation: 24,
                                borderRadius: BorderRadius.circular(16.0),
                                clipBehavior: Clip.antiAlias,
                                child: NeovimEditorOverlay(
                                  onClose: _controller.toggleEditor,
                                ),
                              ),
                            ),
                          ),
                        ),
                      ),
                  ],
                ),
              );
            }
          ),
        ),
      ),
    );
  }
}
