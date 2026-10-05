import 'dart:typed_data';

import 'rust/api/dupelens.dart' as rust;
import 'service.dart';

/// [DupeLensService] backed by the Rust core via flutter_rust_bridge.
///
/// `lib/src/rust/` is generated: run `flutter_rust_bridge_codegen generate` in app/.
class RustDupeLensService implements DupeLensService {
  RustDupeLensService._(this._engine);

  final rust.DupeLens _engine;

  /// Call after `RustLib.init()`.
  static RustDupeLensService open(String dbPath) =>
      RustDupeLensService._(rust.DupeLens.open(dbPath: dbPath));

  @override
  bool get trashSupported => rust.DupeLens.trashSupported();

  @override
  Future<List<String>> folders() => _engine.folders();

  @override
  Future<String> addFolder(String path) => _engine.addFolder(path: path);

  @override
  Future<bool> removeFolder(String path) => _engine.removeFolder(path: path);

  @override
  Stream<ScanEvent> scan() => _engine.scan().map((e) => switch (e) {
        rust.ScanEvent_Progress(:final phase, :final done, :final total) =>
          ScanProgressEvent(ScanProgress(_phase(phase), done, total)),
        rust.ScanEvent_Finished(field0: final s) => ScanDoneEvent(ScanSummary(
            filesFound: s.filesFound,
            filesAnalyzed: s.filesAnalyzed,
            filesUnchanged: s.filesUnchanged,
            filesRemoved: s.filesRemoved,
            unreadableCount: s.unreadableCount,
            missingFolders: s.missingFolders,
          )),
      });

  static ScanPhase _phase(rust.ScanPhaseDto p) => switch (p) {
        rust.ScanPhaseDto.discovering => ScanPhase.discovering,
        rust.ScanPhaseDto.analyzing => ScanPhase.analyzing,
        rust.ScanPhaseDto.saving => ScanPhase.saving,
      };

  @override
  Future<List<DuplicateGroup>> groups() async {
    final groups = await _engine.groups();
    return [
      for (final g in groups)
        DuplicateGroup(
          id: g.id,
          kind: g.kind == rust.GroupKindDto.exact ? GroupKind.exact : GroupKind.near,
          recommendedId: g.recommendedId,
          reason: g.reason,
          reclaimableBytes: g.reclaimableBytes,
          members: [
            for (final f in g.members)
              PhotoFile(
                id: f.id,
                path: f.path,
                sizeBytes: f.sizeBytes,
                width: f.width,
                height: f.height,
                takenAt: f.takenAt,
                favorite: f.favorite,
                identicalToRecommended: f.identicalToRecommended,
                distanceToRecommended: f.distanceToRecommended,
              ),
          ],
        ),
    ];
  }

  @override
  Future<DeleteResult> delete(List<int> fileIds, {required bool permanent}) async {
    final r = await _engine.delete(
      fileIds: Int64List.fromList(fileIds),
      permanent: permanent,
    );
    return DeleteResult(
      deletedIds: r.deletedIds.toList(),
      failures: {for (final f in r.failed) f.fileId: f.reason},
      bytesFreed: r.bytesFreed,
    );
  }
}
