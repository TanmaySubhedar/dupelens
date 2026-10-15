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
