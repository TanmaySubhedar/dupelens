import 'package:flutter/foundation.dart';

import 'service.dart';

/// Which files the user has marked for deletion, per duplicate group.
///
/// Starts in the PRD's default state: in every group, everything except the recommended
/// copy is pre-selected. Nothing is deleted until the user confirms in the delete dialog.
class SelectionModel extends ChangeNotifier {
  SelectionModel(List<DuplicateGroup> groups) {
    replaceGroups(groups);
  }

  List<DuplicateGroup> _groups = const [];
  final Map<int, Set<int>> _selected = {};

  List<DuplicateGroup> get groups => _groups;

  /// Loads new scan results, resetting every group to "Keep Recommended".
  void replaceGroups(List<DuplicateGroup> groups) {
    _groups = groups;
    _selected
      ..clear()
      ..addEntries(groups.map((g) => MapEntry(g.id, g.defaultDeletion)));
    notifyListeners();
  }

  bool isSelected(DuplicateGroup group, PhotoFile file) =>
      _selected[group.id]?.contains(file.id) ?? false;

  Set<int> selectedIn(DuplicateGroup group) =>
      Set.unmodifiable(_selected[group.id] ?? const <int>{});

  /// Manual selection. Returns false (and changes nothing) if this would mark every copy
  /// in the group, since at least one copy must always be kept.
  bool toggle(DuplicateGroup group, PhotoFile file) {
    final set = _selected.putIfAbsent(group.id, () => <int>{});
    if (set.contains(file.id)) {
      set.remove(file.id);
    } else {
      if (set.length + 1 >= group.members.length) return false;
      set.add(file.id);
    }
    notifyListeners();
    return true;
  }

  /// Per-group "Restore Group": back to the recommended default.
  void restoreGroup(DuplicateGroup group) {
    _selected[group.id] = group.defaultDeletion;
    notifyListeners();
  }

  /// Per-group "Clear Selection": keep every copy in this group.
  void clearGroup(DuplicateGroup group) {
    _selected[group.id] = <int>{};
    notifyListeners();
  }

  /// Global "Keep Recommended" batch action.
  void keepRecommendedEverywhere() {
    for (final g in _groups) {
      _selected[g.id] = g.defaultDeletion;
    }
    notifyListeners();
  }

  /// Global "Clear Selection" batch action.
  void clearAll() {
    for (final g in _groups) {
      _selected[g.id] = <int>{};
    }
    notifyListeners();
  }

  /// Every file currently marked for deletion, in group order.
  List<PhotoFile> get selectedFiles => [
        for (final g in _groups)
          for (final m in g.members)
            if (_selected[g.id]?.contains(m.id) ?? false) m,
      ];

  int get selectedBytes =>
      selectedFiles.fold(0, (sum, f) => sum + f.sizeBytes);

  /// Removes deleted files from the results after a delete. Groups left with a single file
  /// are no longer duplicates and disappear.
  void applyDeletion(Iterable<int> deletedIds) {
    final gone = deletedIds.toSet();
    final remaining = <DuplicateGroup>[];
    for (final g in _groups) {
      final members = g.members.where((m) => !gone.contains(m.id)).toList();
      if (members.length < 2) {
        _selected.remove(g.id);
        continue;
      }
      remaining.add(DuplicateGroup(
        id: g.id,
        kind: g.kind,
        members: members,
        recommendedId: g.recommendedId,
        reason: g.reason,
        reclaimableBytes: members
            .where((m) => m.id != g.recommendedId)
            .fold(0, (s, m) => s + m.sizeBytes),
      ));
      _selected[g.id]?.removeAll(gone);
    }
    _groups = remaining;
    notifyListeners();
  }
}
