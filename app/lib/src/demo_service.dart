import 'service.dart';

/// In-memory stand-in for the engine with made-up photos, so the UI can be run and
/// designed before the Rust bridge is generated: `flutter run -t lib/main_demo.dart`.
class DemoDupeLensService implements DupeLensService {
  final List<String> _folders = [];
  final Set<int> _deleted = {};

  @override
  bool get trashSupported => true;

  @override
  Future<List<String>> folders() async => List.of(_folders);

  @override
  Future<String> addFolder(String path) async {
    if (!_folders.contains(path)) _folders.add(path);
    return path;
  }

  @override
  Future<bool> removeFolder(String path) async => _folders.remove(path);

  @override
  Stream<ScanEvent> scan() async* {
    const total = 240;
    yield const ScanProgressEvent(ScanProgress(ScanPhase.discovering, 0, 0));
    for (var done = 0; done <= total; done += 12) {
      await Future<void>.delayed(const Duration(milliseconds: 40));
      yield ScanProgressEvent(ScanProgress(ScanPhase.analyzing, done, total));
    }
    yield const ScanDoneEvent(ScanSummary(
      filesFound: total,
      filesAnalyzed: total,
      filesUnchanged: 0,
      filesRemoved: 0,
      unreadableCount: 0,
      missingFolders: [],
    ));
  }

  @override
  Future<List<DuplicateGroup>> groups() async {
    final all = [
      DuplicateGroup(
        id: 1,
        kind: GroupKind.near,
        recommendedId: 1,
        reason: 'Highest resolution (4032 × 3024)',
        reclaimableBytes: 4200000 + 380000,
        members: const [
          PhotoFile(id: 1, path: '/Pictures/Camera/IMG_2041.jpg', sizeBytes: 4200000, width: 4032, height: 3024, takenAt: '2025-07-14 18:02:11', identicalToRecommended: true, distanceToRecommended: 0),
          PhotoFile(id: 2, path: '/Pictures/Backup/IMG_2041.jpg', sizeBytes: 4200000, width: 4032, height: 3024, takenAt: '2025-07-14 18:02:11', identicalToRecommended: true, distanceToRecommended: 0),
          PhotoFile(id: 3, path: '/Pictures/WhatsApp/IMG-20250714-WA0003.jpg', sizeBytes: 380000, width: 1600, height: 1200, distanceToRecommended: 2),
        ],
      ),
      DuplicateGroup(
        id: 4,
        kind: GroupKind.exact,
        recommendedId: 4,
        reason: 'All copies are identical; keeping the oldest file',
        reclaimableBytes: 2900000,
        members: const [
          PhotoFile(id: 4, path: '/Pictures/Camera/IMG_1877.jpg', sizeBytes: 2900000, width: 3024, height: 4032, identicalToRecommended: true, distanceToRecommended: 0),
          PhotoFile(id: 5, path: '/Pictures/Downloads/IMG_1877 (1).jpg', sizeBytes: 2900000, width: 3024, height: 4032, identicalToRecommended: true, distanceToRecommended: 0),
        ],
      ),
    ];
    return [
      for (final g in all)
        if (g.members.where((m) => !_deleted.contains(m.id)).length > 1) g,
    ];
  }

  @override
  Future<DeleteResult> delete(List<int> fileIds, {required bool permanent}) async {
    _deleted.addAll(fileIds);
    return DeleteResult(deletedIds: fileIds, failures: const {}, bytesFreed: fileIds.length * 1000000);
  }
}
