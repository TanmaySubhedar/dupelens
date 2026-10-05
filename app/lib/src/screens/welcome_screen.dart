import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';

import '../service.dart';
import 'scan_screen.dart';

/// First launch, and later "Manage folders": the user chooses exactly which folders
/// DupeLens may look at. Nothing else on the device is ever read.
class WelcomeScreen extends StatefulWidget {
  const WelcomeScreen({super.key, required this.service, this.firstLaunch = true});

  final DupeLensService service;
  final bool firstLaunch;

  @override
  State<WelcomeScreen> createState() => _WelcomeScreenState();
}

class _WelcomeScreenState extends State<WelcomeScreen> {
  List<String> _folders = [];

  @override
  void initState() {
    super.initState();
    _reload();
  }

  Future<void> _reload() async {
    final folders = await widget.service.folders();
    if (mounted) setState(() => _folders = folders);
  }

  Future<void> _addFolder() async {
    final path = await FilePicker.platform.getDirectoryPath(
      dialogTitle: 'Choose a folder with photos',
    );
    if (path == null) return;
    try {
      await widget.service.addFolder(path);
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('Could not add folder: $e')));
      }
    }
    await _reload();
  }

  Future<void> _removeFolder(String path) async {
    await widget.service.removeFolder(path);
    await _reload();
  }

  void _startScan() {
    Navigator.of(context).pushAndRemoveUntil(
      MaterialPageRoute(builder: (_) => ScanScreen(service: widget.service)),
      (_) => false,
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: widget.firstLaunch ? null : AppBar(title: const Text('Folders')),
      body: SafeArea(
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 560),
            child: ListView(
              padding: const EdgeInsets.all(24),
              children: [
                if (widget.firstLaunch) ...[
                  const SizedBox(height: 32),
                  Icon(Icons.photo_library_outlined, size: 64, color: theme.colorScheme.primary),
                  const SizedBox(height: 16),
                  Text('Welcome to DupeLens',
                      style: theme.textTheme.headlineMedium, textAlign: TextAlign.center),
                  const SizedBox(height: 12),
                  Text(
                    'Find duplicate photos and free up space. Everything happens on this '
                    'device: your photos are never uploaded, and nothing is deleted '
                    'without your confirmation.',
                    style: theme.textTheme.bodyLarge,
                    textAlign: TextAlign.center,
                  ),
                  const SizedBox(height: 32),
                ],
                Text('Folders to scan', style: theme.textTheme.titleMedium),
                const SizedBox(height: 4),
                Text('DupeLens only looks inside the folders you choose.',
                    style: theme.textTheme.bodySmall),
                const SizedBox(height: 12),
                for (final f in _folders)
                  Card(
                    child: ListTile(
                      leading: const Icon(Icons.folder_outlined),
                      title: Text(f, overflow: TextOverflow.ellipsis),
                      trailing: IconButton(
                        tooltip: 'Stop scanning this folder',
                        icon: const Icon(Icons.close),
                        onPressed: () => _removeFolder(f),
                      ),
                    ),
                  ),
                OutlinedButton.icon(
                  onPressed: _addFolder,
                  icon: const Icon(Icons.create_new_folder_outlined),
                  label: Text(_folders.isEmpty ? 'Choose a folder' : 'Add another folder'),
                ),
                const SizedBox(height: 24),
                FilledButton.icon(
                  onPressed: _folders.isEmpty ? null : _startScan,
                  icon: const Icon(Icons.search),
                  label: const Text('Find duplicates'),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
