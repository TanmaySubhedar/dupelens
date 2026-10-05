import 'dart:io';

import 'package:flutter/material.dart';

import '../selection.dart';
import '../service.dart';

/// One duplicate group as a photo grid. The recommended copy wears a crown; copies marked
/// for deletion are dimmed with a check mark. Tap toggles, long-press shows details.
class GroupCard extends StatelessWidget {
  const GroupCard({super.key, required this.group, required this.selection});

  final DuplicateGroup group;
  final SelectionModel selection;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final selected = selection.selectedIn(group);
    return Card(
      clipBehavior: Clip.antiAlias,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(12, 4, 4, 12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(children: [
              Expanded(
                child: Text(
                  '${group.kind == GroupKind.exact ? 'Exact copies' : 'Look alike'} · '
                  '${group.members.length} photos',
                  style: theme.textTheme.titleSmall,
                ),
              ),
              if (selected.isNotEmpty)
                Text('${selected.length} to delete', style: theme.textTheme.bodySmall),
              PopupMenuButton<String>(
                tooltip: 'Group options',
                onSelected: (v) => switch (v) {
                  'restore' => selection.restoreGroup(group),
                  _ => selection.clearGroup(group),
                },
                itemBuilder: (_) => const [
                  PopupMenuItem(value: 'restore', child: Text('Restore group (keep 👑 only)')),
                  PopupMenuItem(value: 'clear', child: Text('Clear selection (keep all)')),
                ],
              ),
            ]),
            Padding(
              padding: const EdgeInsets.only(right: 8, bottom: 8),
              child: Text('👑 ${group.reason}', style: theme.textTheme.bodySmall),
            ),
            LayoutBuilder(builder: (context, c) {
              final columns = (c.maxWidth / 150).floor().clamp(2, 6);
              return GridView.count(
                crossAxisCount: columns,
                shrinkWrap: true,
                physics: const NeverScrollableScrollPhysics(),
                mainAxisSpacing: 6,
                crossAxisSpacing: 6,
                padding: const EdgeInsets.only(right: 8),
                children: [
                  for (final f in group.members)
                    _PhotoTile(
                      file: f,
                      recommended: f.id == group.recommendedId,
                      selected: selected.contains(f.id),
                      onTap: () {
                        if (!selection.toggle(group, f)) {
                          ScaffoldMessenger.of(context).showSnackBar(const SnackBar(
                            content: Text('Keep at least one copy of every photo.'),
                          ));
                        }
                      },
                      onLongPress: () => showPhotoDetails(context, f, group),
                    ),
                ],
              );
            }),
          ],
        ),
      ),
    );
  }
}

class _PhotoTile extends StatelessWidget {
  const _PhotoTile({
    required this.file,
    required this.recommended,
    required this.selected,
    required this.onTap,
    required this.onLongPress,
  });

  final PhotoFile file;
  final bool recommended;
  final bool selected;
  final VoidCallback onTap;
  final VoidCallback onLongPress;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Semantics(
      label: '${file.name}${recommended ? ', recommended' : ''}'
          '${selected ? ', marked for deletion' : ''}',
      button: true,
      child: GestureDetector(
        onTap: onTap,
        onLongPress: onLongPress,
        child: ClipRRect(
          borderRadius: BorderRadius.circular(8),
          child: Stack(fit: StackFit.expand, children: [
            PhotoThumbnail(path: file.path),
            if (selected) Container(color: Colors.black.withValues(alpha: 0.45)),
            if (recommended)
              const Positioned(
                left: 6,
                top: 4,
                child: Text('👑', style: TextStyle(fontSize: 20)),
              ),
            Positioned(
              right: 6,
              top: 6,
              child: Icon(
                selected ? Icons.check_circle : Icons.radio_button_unchecked,
                color: selected ? scheme.error : Colors.white,
                shadows: const [Shadow(blurRadius: 4)],
              ),
            ),
          ]),
        ),
      ),
    );
  }
}

class PhotoThumbnail extends StatelessWidget {
  const PhotoThumbnail({super.key, required this.path});

  final String path;

  @override
  Widget build(BuildContext context) {
    return Image.file(
      File(path),
      fit: BoxFit.cover,
      cacheWidth: 360, // decode a thumbnail, not the full photo
      errorBuilder: (context, _, __) => Container(
        color: Theme.of(context).colorScheme.surfaceContainerHighest,
        child: const Icon(Icons.image_outlined, size: 36),
      ),
    );
  }
}

/// Technical details, shown only on demand.
void showPhotoDetails(BuildContext context, PhotoFile f, DuplicateGroup group) {
  final rows = <(String, String)>[
    ('File', f.path),
    ('Size', formatBytes(f.sizeBytes)),
    if (f.dimensions != null) ('Dimensions', f.dimensions!),
    if (f.takenAt != null) ('Taken', f.takenAt!),
    if (f.favorite) ('Rating', 'Favourite'),
    (
      'Match',
      f.id == group.recommendedId
          ? 'Recommended copy'
          : f.identicalToRecommended
              ? 'Byte-for-byte identical to 👑'
              : 'Looks the same as 👑 (difference ${f.distanceToRecommended ?? '?'} of 64)'
    ),
  ];
  showModalBottomSheet<void>(
    context: context,
    showDragHandle: true,
    builder: (context) => SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(24, 0, 24, 24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            for (final (label, value) in rows)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 4),
                child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  SizedBox(
                      width: 96,
                      child: Text(label, style: Theme.of(context).textTheme.labelLarge)),
                  Expanded(child: SelectableText(value)),
                ]),
              ),
          ],
        ),
      ),
    ),
  );
}
