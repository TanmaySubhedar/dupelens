import test from 'node:test';
import assert from 'node:assert/strict';
import { byCategory, formatBytes, nextMilestone } from '../js/format.js';

test('byCategory totals groups, files and reclaimable bytes per type, skipping empty types', () => {
  const g = (category, n, bytes) => ({ category, members: new Array(n).fill({}), reclaimable_bytes: bytes });
  const rows = byCategory([g('photo', 2, 10), g('photo', 3, 5), g('video', 2, 100)]);
  assert.deepEqual(rows, [
    { key: 'photo', groups: 2, files: 5, bytes: 15 },
    { key: 'video', groups: 1, files: 2, bytes: 100 },
  ]);
});

test('nextMilestone picks the next round total', () => {
  const gb = 1024 ** 3;
  assert.equal(nextMilestone(0), gb);
  assert.equal(nextMilestone(gb), 5 * gb);
  assert.equal(nextMilestone(30 * gb), 50 * gb);
  assert.equal(nextMilestone(2000 * gb), null);
  assert.equal(formatBytes(gb), '1.0 GB');
});
