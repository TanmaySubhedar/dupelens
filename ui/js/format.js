// Plain-language formatting shared by the screens.

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

/** 1536 -> "1.5 KB". Same rules as the CLI's human_bytes. */
export function formatBytes(n) {
  let v = n;
  let u = 0;
  while (v >= 1024 && u < UNITS.length - 1) {
    v /= 1024;
    u++;
  }
  return u === 0 ? `${n} B` : `${v.toFixed(1)} ${UNITS[u]}`;
}

/** plural(3, 'photo') -> "3 photos". */
export function plural(n, word, many = `${word}s`) {
  return `${n.toLocaleString()} ${n === 1 ? word : many}`;
}

export function dimensions(file) {
  return file.width && file.height ? `${file.width} × ${file.height}` : null;
}

/** The file types the engine reports, in the order the pages list them. */
export const CATEGORIES = ['photo', 'video', 'music', 'document', 'archive', 'other'];
const CATEGORY_NAMES = { photo: 'Photos', video: 'Videos', music: 'Music', document: 'Documents', archive: 'Archives', other: 'Other' };

export function categoryName(key) {
  return CATEGORY_NAMES[key] ?? 'Other';
}

/** Per file type: how many groups and files, and how much space removing the extras frees. */
export function byCategory(groups) {
  const rows = new Map(CATEGORIES.map((c) => [c, { key: c, groups: 0, files: 0, bytes: 0 }]));
  for (const g of groups) {
    const row = rows.get(g.category) ?? rows.get('other');
    row.groups += 1;
    row.files += g.members.length;
    row.bytes += g.reclaimable_bytes;
  }
  return [...rows.values()].filter((r) => r.groups > 0);
}

const MILESTONES_GB = [1, 5, 10, 25, 50, 100, 250, 500, 1024];

/** The next round total to aim for above `bytes`, or null past the last one. */
export function nextMilestone(bytes) {
  const gb = 1024 ** 3;
  const next = MILESTONES_GB.find((m) => m * gb > bytes);
  return next == null ? null : next * gb;
}
