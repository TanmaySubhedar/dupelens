// Run with: node --test ui/tests/*.test.js
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { defaultDeletion, Selection } from '../js/selection.js';

const file = (id, size = 100) => ({ id, size, name: `${id}.jpg` });
const group = (id, recommended, ...ids) => ({ id, recommended_id: recommended, members: ids.map((i) => file(i)) });

test('starts with everything except the recommended copy marked', () => {
  const g = group(1, 2, 1, 2, 3);
  const s = new Selection([g]);
  assert.deepEqual([...s.markedIn(g)].sort(), [1, 3]);
  assert.deepEqual([...defaultDeletion(g)].sort(), [1, 3]);
  assert.equal(s.markedBytes(), 200);
});

test('refuses to mark every copy', () => {
  const g = group(1, 1, 1, 2);
  const s = new Selection([g]);
  assert.equal(s.toggle(g, 1), false, 'marking the last kept copy is refused');
  assert.equal(s.isMarked(g, 1), false);
  assert.equal(s.toggle(g, 2), true, 'unmarking is always allowed');
  assert.equal(s.toggle(g, 1), true);
  assert.deepEqual(s.markedFiles().map(({ file: f }) => f.id), [1]);
});

test('restore, keep all and batch actions', () => {
  const a = group(1, 1, 1, 2);
  const b = group(3, 3, 3, 4, 5);
  const s = new Selection([a, b]);
  s.keepAll(a);
  assert.equal(s.markedIn(a).size, 0);
  s.restore(a);
  assert.deepEqual([...s.markedIn(a)], [2]);
  s.clear([b]);
  assert.equal(s.markedFiles().length, 1, 'only the given groups are cleared');
  s.keepRecommended();
  assert.equal(s.markedFiles().length, 3);
});

test('reloading after a partial delete keeps earlier choices', () => {
  const g = group(1, 1, 1, 2, 3, 4);
  const s = new Selection([g]);
  s.toggle(g, 4); // keep 4: marked = {2, 3}
  // 2 was deleted; 3 failed and is still there.
  s.load([group(1, 1, 1, 3, 4)], { keepPrevious: true });
  assert.deepEqual([...s.marked.get(1)], [3]);
  // A brand-new group gets the default.
  s.load([group(1, 1, 1, 3, 4), group(9, 9, 9, 10)], { keepPrevious: true });
  assert.deepEqual([...s.marked.get(9)], [10]);
});

test('a reload never leaves every copy marked', () => {
  const g = group(1, 1, 1, 2, 3);
  const s = new Selection([g]);
  // The recommended copy 1 vanished; the old marks {2, 3} would cover every remaining copy.
  s.load([group(1, 2, 2, 3)], { keepPrevious: true });
  assert.deepEqual([...s.marked.get(1)], [3]);
});
