import 'package:dupelens/src/selection.dart';
import 'package:dupelens/src/service.dart';
import 'package:flutter_test/flutter_test.dart';

PhotoFile photo(int id, {int size = 100}) =>
    PhotoFile(id: id, path: '/photos/$id.jpg', sizeBytes: size);

DuplicateGroup group(int id, List<int> memberIds, {required int keep}) =>
    DuplicateGroup(
      id: id,
      kind: GroupKind.near,
      members: [for (final m in memberIds) photo(m)],
      recommendedId: keep,
      reason: 'test',
      reclaimableBytes: 100 * (memberIds.length - 1),
    );

void main() {
  late DuplicateGroup a;
  late DuplicateGroup b;
  late SelectionModel model;

  setUp(() {
    a = group(1, [1, 2, 3], keep: 2);
    b = group(4, [4, 5], keep: 4);
    model = SelectionModel([a, b]);
  });

  test('starts with everything except the recommended copy selected', () {
    expect(model.selectedIn(a), {1, 3});
    expect(model.selectedIn(b), {5});
    expect(model.selectedBytes, 300);
  });

  test('cannot select every copy in a group', () {
    expect(model.toggle(a, a.members[1]), isFalse); // would select 1, 2, 3
    expect(model.selectedIn(a), {1, 3});
  });

  test('manual toggle, clear and restore', () {
    expect(model.toggle(a, a.members[0]), isTrue); // unselect 1
    expect(model.selectedIn(a), {3});
    model.clearGroup(a);
    expect(model.selectedIn(a), isEmpty);
    model.restoreGroup(a);
    expect(model.selectedIn(a), {1, 3});
  });

  test('global batch actions', () {
    model.clearAll();
    expect(model.selectedFiles, isEmpty);
    model.keepRecommendedEverywhere();
    expect(model.selectedFiles.map((f) => f.id), [1, 3, 5]);
  });

  test('applyDeletion drops resolved groups', () {
    model.applyDeletion([1, 3, 5]);
    expect(model.groups, isEmpty);
  });

  test('applyDeletion keeps partially cleaned groups', () {
    model.applyDeletion([1]);
    expect(model.groups.map((g) => g.id), [a.id, b.id]);
    expect(model.groups.first.members.map((m) => m.id), [2, 3]);
    expect(model.selectedIn(model.groups.first), {3});
  });
}
