// The only file that knows how the page reaches the Rust backend. Everything else calls
// `call(cmd, args)`, `thumbUrl(id)`, `pickFolder()` and `quit()`.
//
// The host is the Tauri app (docs/UI.md): `invoke('api', { cmd, args })`, with previews from
// the `thumb` URI scheme. Needs `app.withGlobalTauri: true` in tauri.conf.json.

const tauri = globalThis.__TAURI__?.core ?? null;

export class ApiError extends Error {
  /** kind: 'bad_request' | 'not_found' | 'busy' | 'failed' | 'offline' */
  constructor(kind, message) {
    super(message);
    this.kind = kind;
  }
}

export async function call(cmd, args = {}) {
  if (!tauri) throw new ApiError('offline', 'DupeLens must be opened as the app.');
  try {
    return await tauri.invoke('api', { cmd, args });
  } catch (e) {
    throw new ApiError(e?.kind ?? 'failed', e?.message ?? String(e));
  }
}

/** Address of a photo's preview. `large` for the enlarged view. */
export function thumbUrl(id, large = false) {
  const size = large ? 'size=large' : 'size=small';
  return `${tauri.convertFileSrc(String(id), 'thumb')}?${size}`;
}

/** Opens the system folder dialog. Resolves to a path, or null if cancelled/unavailable. */
export function pickFolder() {
  return call('pick_folder');
}

/** Stops the app by closing its window. */
export function quit() {
  return call('quit');
}
