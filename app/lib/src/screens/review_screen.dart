import 'package:flutter/material.dart';

import '../selection.dart';
import '../service.dart';
import '../widgets/group_card.dart';

/// "Review Before Deleting": shows exactly which photos will go, then asks how to delete.
/// Pops with the ids that were actually deleted.
class ReviewScreen extends StatefulWidget {
  const ReviewScreen({super.key, required this.service, required this.selection});

  final DupeLensService service;
  final SelectionModel selection;

  @override
  State<ReviewScreen> createState() => _ReviewScreenState();
}

class _ReviewScreenState extends State<ReviewScreen> {
  bool _deleting = false;

  Future<void> _confirmAndDelete() async {
    final files = widget.selection.selectedFiles;
    final permanent = await showDialog<bool>(
      context: context,
      builder: (_) => _DeleteDialog(
        count: files.length,
        bytes: widget.selection.selectedBytes,
        trashSupported: widget.service.trashSupported,
      ),
    );
    if (permanent == null || !mounted) return; // cancelled

    setState(() => _deleting = true);
    final messenger = ScaffoldMessenger.of(context);
    final navigator = Navigator.of(context);
    try {
      final result = await widget.service.delete(
        [for (final f in files) f.id],
        permanent: permanent,
      );
      final skipped = result.failures.length;
      messenger.showSnackBar(SnackBar(
        content: Text(
          'Freed ${formatBytes(result.bytesFreed)} '
          '(${result.deletedIds.length} photos${permanent ? ' deleted' : ' moved to the bin'})'
          '${skipped > 0 ? '. $skipped skipped because they changed since the scan; scan again.' : '.'}',
        ),
      ));
      navigator.pop(result.deletedIds);
    } catch (e) {
      setState(() => _deleting = false);
      messenger.showSnackBar(SnackBar(content: Text('Nothing was deleted: $e')));
    }
  }

  @override
  Widget build(BuildContext context) {
    final files = widget.selection.selectedFiles;
    return Scaffold(
      appBar: AppBar(title: Text('Review ${files.length} photos')),
      body: GridView.extent(
        maxCrossAxisExtent: 160,
        padding: const EdgeInsets.fromLTRB(12, 12, 12, 96),
        mainAxisSpacing: 6,
        crossAxisSpacing: 6,
        children: [
          for (final f in files)
            ClipRRect(
              borderRadius: BorderRadius.circular(8),
              child: Tooltip(message: f.path, child: PhotoThumbnail(path: f.path)),
            ),
        ],
      ),
      floatingActionButtonLocation: FloatingActionButtonLocation.centerFloat,
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _deleting || files.isEmpty ? null : _confirmAndDelete,
        icon: _deleting
            ? const SizedBox.square(dimension: 20, child: CircularProgressIndicator(strokeWidth: 2))
            : const Icon(Icons.delete_outline),
        label: Text('Delete ${files.length} · ${formatBytes(widget.selection.selectedBytes)}'),
      ),
    );
  }
}

/// Returns false for "move to trash", true for "permanently delete", null for cancel.
class _DeleteDialog extends StatefulWidget {
  const _DeleteDialog({required this.count, required this.bytes, required this.trashSupported});

  final int count;
  final int bytes;
  final bool trashSupported;

  @override
  State<_DeleteDialog> createState() => _DeleteDialogState();
}

class _DeleteDialogState extends State<_DeleteDialog> {
  late bool _permanent = !widget.trashSupported;

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Text('Delete ${widget.count} photos?'),
      content: Column(mainAxisSize: MainAxisSize.min, children: [
        Text('This frees ${formatBytes(widget.bytes)}. The recommended copy of each photo is kept.'),
        const SizedBox(height: 8),
        RadioGroup<bool>(
          groupValue: _permanent,
          onChanged: (v) => setState(() => _permanent = v ?? _permanent),
          child: Column(children: [
            if (widget.trashSupported)
              const RadioListTile<bool>(
                value: false,
                title: Text('Move to Recycle Bin'),
                subtitle: Text('You can restore them later'),
              ),
            const RadioListTile<bool>(
              value: true,
              title: Text('Delete permanently'),
              subtitle: Text('Cannot be undone'),
            ),
          ]),
        ),
      ]),
      actions: [
        TextButton(onPressed: () => Navigator.pop(context), child: const Text('Cancel')),
        FilledButton(
          style: _permanent
              ? FilledButton.styleFrom(backgroundColor: Theme.of(context).colorScheme.error)
              : null,
          onPressed: () => Navigator.pop(context, _permanent),
          child: Text(_permanent ? 'Delete permanently' : 'Move to bin'),
        ),
      ],
    );
  }
}
