import 'dart:io';
import 'package:flutter/material.dart';
import 'package:xterm/xterm.dart';
import 'package:pty/pty.dart';

class NeovimEditorOverlay extends StatefulWidget {
  final VoidCallback onClose;
  
  const NeovimEditorOverlay({super.key, required this.onClose});

  @override
  State<NeovimEditorOverlay> createState() => _NeovimEditorOverlayState();
}

class _NeovimEditorOverlayState extends State<NeovimEditorOverlay> {
  final terminal = Terminal();
  PseudoTerminal? pty;
  final TerminalController terminalController = TerminalController();

  @override
  void initState() {
    super.initState();
    _startPty();
  }
  
  void _startPty() {
    try {
      // Start neovim process
      final newPty = PseudoTerminal.start(
        'nvim',
        [],
        environment: Platform.environment,
      );
      pty = newPty;

      newPty.out.listen((data) {
        terminal.write(data);
      });

      terminal.onOutput = (data) {
        newPty.write(data);
      };
      
      terminal.onResize = (width, height, pixelWidth, pixelHeight) {
        newPty.resize(width, height);
      };
      
      // If the pty exits, close the overlay
      newPty.exitCode.then((_) {
        if (mounted) {
          widget.onClose();
        }
      });
    } catch (e) {
      terminal.write('Failed to start Neovim: $e\r\n');
    }
  }

  @override
  void dispose() {
    pty?.kill();
    terminalController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      color: Colors.black.withValues(alpha: 0.9),
      padding: const EdgeInsets.all(16.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              const Text(
                'Integrated Editor (Neovim)',
                style: TextStyle(color: Colors.white, fontWeight: FontWeight.bold, fontSize: 16),
              ),
              IconButton(
                icon: const Icon(Icons.close, color: Colors.white),
                onPressed: widget.onClose,
                tooltip: 'Close Editor',
              ),
            ],
          ),
          const SizedBox(height: 8),
          Expanded(
            child: ClipRRect(
              borderRadius: BorderRadius.circular(8.0),
              child: TerminalView(
                terminal,
                controller: terminalController,
                autofocus: true,
                backgroundOpacity: 1.0,
                theme: TerminalThemes.defaultTheme,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
