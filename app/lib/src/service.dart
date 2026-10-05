/// The app's view of the engine. The UI depends only on this file, so it can run against
/// the real Rust engine (rust_service.dart) or demo data (demo_service.dart).
library;

enum GroupKind { exact, near }

class PhotoFile {
  const PhotoFile({
    required this.id,
    required this.path,
    required this.sizeBytes,
    this.width,
    this.height,
    this.takenAt,
    this.favorite = false,
    this.identicalToRecommended = false,
    this.distanceToRecommended,
  });

  final int id;
  final String path;
  final int sizeBytes;
  final int? width;
  final int? height;
  final String? takenAt;
  final bool favorite;
  final bool identicalToRecommended;
  final int? distanceToRecommended;

  String get name => path.split(RegExp(r'[\\/]')).last;
  String? get dimensions =>
      width != null && height != null ? '$width × $height' : null;
}

class DuplicateGroup {
  const DuplicateGroup({
    required this.id,
    required this.kind,
    required this.members,
    required this.recommendedId,
    required this.reason,
    required this.reclaimableBytes,
  });

  final int id;
  final GroupKind kind;
  final List<PhotoFile> members;
  final int recommendedId;
  final String reason;
  final int reclaimableBytes;

  /// "Keep Recommended": every member except the recommended one.
  Set<int> get defaultDeletion =>
      {for (final m in members) if (m.id != recommendedId) m.id};
}

enum ScanPhase { discovering, analyzing, saving }

class ScanProgress {
  const ScanProgress(this.phase, this.done, this.total);
  final ScanPhase phase;
  final int done;
  final int total;
}

class ScanSummary {
  const ScanSummary({
    required this.filesFound,
    required this.filesAnalyzed,
    required this.filesUnchanged,
    required this.filesRemoved,
    required this.unreadableCount,
    required this.missingFolders,
  });

  final int filesFound;
  final int filesAnalyzed;
  final int filesUnchanged;
  final int filesRemoved;
  final int unreadableCount;
  final List<String> missingFolders;
}

/// Emitted by [DupeLensService.scan]: zero or more [ScanProgressEvent]s, then one [ScanDoneEvent].
sealed class ScanEvent {
  const ScanEvent();
}

class ScanProgressEvent extends ScanEvent {
  const ScanProgressEvent(this.progress);
  final ScanProgress progress;
}

class ScanDoneEvent extends ScanEvent {
  const ScanDoneEvent(this.summary);
  final ScanSummary summary;
}

class DeleteResult {
  const DeleteResult({
    required this.deletedIds,
    required this.failures,
    required this.bytesFreed,
  });

  final List<int> deletedIds;

  /// File id → plain-language reason it was skipped.
  final Map<int, String> failures;
  final int bytesFreed;
}

abstract interface class DupeLensService {
  bool get trashSupported;
  Future<List<String>> folders();
  Future<String> addFolder(String path);
  Future<bool> removeFolder(String path);
  Stream<ScanEvent> scan();
  Future<List<DuplicateGroup>> groups();

  /// Deletes exactly [fileIds]. Only call after explicit user confirmation.
  Future<DeleteResult> delete(List<int> fileIds, {required bool permanent});
}

String formatBytes(int bytes) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  var value = bytes.toDouble();
  var unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return unit == 0 ? '$bytes B' : '${value.toStringAsFixed(1)} ${units[unit]}';
}
