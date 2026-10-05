import 'package:flutter/material.dart';

import '../selection.dart';
import '../service.dart';
import '../widgets/group_card.dart';
import 'review_screen.dart';
import 'scan_screen.dart';
import 'welcome_screen.dart';

class ResultsScreen extends StatefulWidget {
  const ResultsScreen({
    super.key,
    required this.service,
    required this.groups,
    required this.summary,
  });

  final DupeLensService service;
  final List<DuplicateGroup> groups;
  final ScanSummary summary;

  @override
  State<ResultsScreen> createState() => _ResultsScreenState();
}

class _ResultsScreenState extends State<ResultsScreen> {
  late final SelectionModel _selection = SelectionModel(widget.groups);

  @override
  void dispose() {
    _selection.dispose();
    super.dispose();
  }

  void _rescan() {
    Navigator.of(context).pushReplacement(
      MaterialPageRoute(builder: (_) => ScanScreen(service: widget.service)),
    );
  }

  Future<void> _review() async {
    final deleted = await Navigator.of(context).push<List<int>>(MaterialPageRoute(
      builder: (_) => ReviewScreen(service: widget.service, selection: _selection),
    ));
    if (deleted != null) _selection.applyDeletion(deleted);
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: _selection,
      builder: (context, _) {
        final groups = _selection.groups;
        final selectedCount = _selection.selectedFiles.length;
        final reclaimable = groups.fold(0, (s, g) => s + g.reclaimableBytes);
        return Scaffold(
          appBar: AppBar(
            title: const Text('DupeLens'),
            actions: [
              IconButton(
                tooltip: 'Folders',
                icon: const Icon(Icons.folder_outlined),
                onPressed: () => Navigator.of(context).push(MaterialPageRoute(
                  builder: (_) => WelcomeScreen(service: widget.service, firstLaunch: false),
                )),
              ),
              IconButton(tooltip: 'Scan again', icon: const Icon(Icons.refresh), onPressed: _rescan),
              PopupMenuButton<String>(
                onSelected: (v) => switch (v) {
                  'keep' => _selection.keepRecommendedEverywhere(),
                  _ => _selection.clearAll(),
                },
                itemBuilder: (_) => const [
                  PopupMenuItem(value: 'keep', child: Text('Keep recommended (all groups)')),
                  PopupMenuItem(value: 'clear', child: Text('Clear selection (all groups)')),
                ],
              ),
            ],
          ),
          body: groups.isEmpty
              ? _Empty(summary: widget.summary)
              : ListView.builder(
                  padding: const EdgeInsets.fromLTRB(12, 8, 12, 96),
                  itemCount: groups.length + 1,
                  itemBuilder: (context, i) => i == 0
                      ? Padding(
                          padding: const EdgeInsets.fromLTRB(4, 4, 4, 12),
                          child: Text(
                            '${groups.length} sets of duplicates · ${formatBytes(reclaimable)} can be freed',
                            style: Theme.of(context).textTheme.titleMedium,
                          ),
                        )
                      : GroupCard(group: groups[i - 1], selection: _selection),
                ),
          floatingActionButtonLocation: FloatingActionButtonLocation.centerFloat,
          floatingActionButton: groups.isEmpty
              ? null
              : FloatingActionButton.extended(
                  onPressed: selectedCount == 0 ? null : _review,
                  icon: const Icon(Icons.delete_outline),
                  label: Text(selectedCount == 0
                      ? 'Nothing selected'
                      : 'Review $selectedCount · ${formatBytes(_selection.selectedBytes)}'),
                ),
        );
      },
    );
  }
}

class _Empty extends StatelessWidget {
  const _Empty({required this.summary});

  final ScanSummary summary;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          Icon(Icons.check_circle_outline, size: 64, color: theme.colorScheme.primary),
          const SizedBox(height: 16),
          Text('No duplicates', style: theme.textTheme.headlineSmall),
          const SizedBox(height: 8),
          Text('Checked ${summary.filesFound} photos.', textAlign: TextAlign.center),
          for (final f in summary.missingFolders)
            Text('Folder not found: $f', style: theme.textTheme.bodySmall),
        ]),
      ),
    );
  }
}
