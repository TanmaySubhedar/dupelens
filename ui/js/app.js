// DupeLens UI: folders -> scan -> results (photo grids) -> review -> done.
// Talks to the backend only through api.js, which forwards to the Tauri app.
// No browser popups (alert/confirm/prompt): some Tauri webviews don't support them, so
// dialogs are drawn in the page.

import { ApiError, call, pickFolder, thumbUrl } from './api.js';
import { h, mount } from './dom.js';
import { dimensions, formatBytes, plural } from './format.js';
import { Selection } from './selection.js';

const main = document.getElementById('main');
const GROUPS_PER_PAGE = 30;
const POLL_MS = 300;

const state = {
  info: { trash_supported: true, platform: '' },
  folders: [],
  groups: [],
  selection: new Selection(),
  filter: 'all', // 'all' | 'exact' | 'near'
  shown: GROUPS_PER_PAGE,
  lastSummary: null,
  settings: null,
  suggested: [],
};

// ---------- small shared pieces ----------

function toast(message, kind = 'info') {
  const el = h('div', { class: `toast ${kind}`, role: kind === 'error' ? 'alert' : 'status' }, message);
  document.getElementById('toasts').append(el);
  setTimeout(() => el.classList.add('leaving'), 4200);
  setTimeout(() => el.remove(), 4600);
}

function showError(e) {
  toast(e instanceof ApiError || e instanceof Error ? e.message : String(e), 'error');
}

/** Runs an async action with its button disabled, reporting errors as toasts. */
async function busy(button, action) {
  button.disabled = true;
  try {
    return await action();
  } catch (e) {
    showError(e);
  } finally {
    button.disabled = false;
  }
}

function photo(file, { large = false, cls = 'thumb' } = {}) {
  const img = h('img', { class: cls, src: thumbUrl(file.id, large), alt: file.name, loading: 'lazy', decoding: 'async' });
  img.addEventListener('error', () => {
    img.replaceWith(h('div', { class: `${cls} no-preview`, title: 'No preview for this format' }, file.name.split('.').pop()?.toUpperCase() ?? '?'));
  }, { once: true });
  return img;
}

/** An in-page modal. Resolves when closed; Escape and the backdrop close it. */
function modal(build) {
  return new Promise((resolve) => {
    const previous = document.activeElement;
    const close = (value) => {
      backdrop.remove();
      document.removeEventListener('keydown', onKey);
      previous?.focus?.();
      resolve(value);
    };
    const onKey = (e) => e.key === 'Escape' && close(null);
    const dialog = h('div', { class: 'dialog', role: 'dialog', 'aria-modal': 'true' }, build(close));
    const backdrop = h('div', { class: 'backdrop', onclick: (e) => e.target === backdrop && close(null) }, dialog);
    document.addEventListener('keydown', onKey);
    document.body.append(backdrop);
    dialog.querySelector('button, input')?.focus();
  });
}

// ---------- screen: folders ----------

async function showFolders() {
  mount(main, h('p', { class: 'muted' }, 'Loading…'));
  try {
    state.folders = await call('folders');
    state.settings = await call('get_settings');
    state.suggested = await call('suggested_folders').catch(() => []);
  } catch (e) {
    showError(e);
  }
  renderFolders();
}

/** First run: one-click suggestions for the folders most people want cleaned. */
function suggestionsSection() {
  const chosen = new Set(state.folders.map((f) => f.toLowerCase()));
  const left = state.suggested.filter((s) => !chosen.has(s.path.toLowerCase()));
  if (state.folders.length > 0 || left.length === 0) return null;
  return h('div', { class: 'suggestions' },
    h('p', { class: 'muted small' }, 'Quick start: add the folders where duplicates usually build up.'),
    h('div', { class: 'chips' }, left.map((s) =>
      h('button', { class: 'secondary', title: s.path, onclick: (e) => busy(e.currentTarget, () => addFolder(s.path)) }, `+ ${s.name}`))));
}

/** "How alike must files be?": the matching preset. Changing it regroups without rescanning. */
function matchingSection() {
  const st = state.settings;
  if (!st) return null;
  const description = h('p', { class: 'muted small' }, st.presets.find((p) => p.key === st.preset)?.description ?? '');
  const select = h('select', {
    'aria-label': 'How alike must files be to count as duplicates',
    onchange: async (e) => {
      const preset = e.currentTarget.value;
      try {
        state.settings = await call('set_settings', { preset });
        if (state.groups.length > 0 || state.lastSummary) await startRegroup();
        else {
          description.textContent = state.settings.presets.find((p) => p.key === preset)?.description ?? '';
          toast('Saved. It applies to your next scan.');
        }
      } catch (err) {
        showError(err);
      }
    },
  }, st.presets
    .filter((p) => p.key !== 'custom' || st.preset === 'custom')
    .map((p) => h('option', { value: p.key, selected: p.key === st.preset }, p.name)));
  return h('div', { class: 'matching' },
    h('h2', {}, 'How alike must files be?'),
    select,
    description,
    h('p', { class: 'muted small' }, 'Changing this updates your results in seconds, without scanning again.'));
}

function renderFolders() {
  const pathInput = h('input', {
    type: 'text', class: 'path-input', placeholder: 'Or paste a folder path',
    'aria-label': 'Folder path', spellcheck: 'false',
  });
  const addTyped = async (button) => {
    if (!pathInput.value.trim()) return pathInput.focus();
    await busy(button, async () => {
      await addFolder(pathInput.value);
      pathInput.value = '';
    });
  };
  const addButton = h('button', { class: 'secondary', onclick: (e) => addTyped(e.currentTarget) }, 'Add');
  pathInput.addEventListener('keydown', (e) => e.key === 'Enter' && addTyped(addButton));

  const scanButton = h('button', { class: 'primary big', disabled: state.folders.length === 0, onclick: (e) => busy(e.currentTarget, startScan) }, 'Scan for duplicates');

  mount(main,
    h('section', { class: 'card narrow' },
      h('h1', {}, 'Find duplicate files'),
      h('p', { class: 'lead' }, 'Choose the folders to look in. DupeLens only reads these folders, works entirely offline, and never deletes anything until you confirm.'),
      h('div', { class: 'folder-actions' },
        h('button', { class: 'primary', onclick: (e) => busy(e.currentTarget, chooseFolder) }, '+ Choose folder…'),
      ),
      suggestionsSection(),
      h('div', { class: 'path-row' }, pathInput, addButton),
      state.folders.length === 0
        ? h('p', { class: 'empty' }, 'No folders yet.')
        : h('ul', { class: 'folder-list' }, state.folders.map((f) =>
          h('li', {},
            h('span', { class: 'folder-icon', 'aria-hidden': 'true' }, '📁'),
            h('span', { class: 'folder-path', title: f }, f),
            h('button', { class: 'icon', title: 'Remove this folder', 'aria-label': `Remove ${f}`, onclick: (e) => busy(e.currentTarget, () => removeFolder(f)) }, '✕'),
          ))),
      matchingSection(),
      h('div', { class: 'scan-row' }, scanButton,
        state.groups.length > 0 && h('button', { class: 'link', onclick: () => showResults() }, `Back to results (${plural(state.groups.length, 'group')})`)),
    ),
  );
}

async function chooseFolder() {
  const path = await pickFolder();
  if (path) await addFolder(path);
}

async function addFolder(path) {
  const stored = await call('add_folder', { path });
  state.folders = await call('folders');
  renderFolders();
  toast(`Added ${stored}`);
}

async function removeFolder(path) {
  await call('remove_folder', { path });
  state.folders = await call('folders');
  renderFolders();
}

// ---------- screen: scanning ----------

const PHASES = {
  discovering: 'Looking for files…',
  analyzing: 'Reading files',
  saving: 'Saving results…',
  comparing: 'Comparing look-alike photos…',
};

async function startScan() {
  await call('start_scan');
  showScan();
}

/** Re-groups the last scan's results with the current matching setting; no folders are re-read. */
async function startRegroup() {
  await call('start_regroup');
  showScan({ title: 'Updating results', note: 'Using what was already scanned, so only new look-alike comparisons take time.' });
}

function showScan({ title = 'Scanning', note = 'Files that haven’t changed since the last scan are skipped, so rescans are quick.' } = {}) {
  const label = h('p', { class: 'phase' }, title === 'Scanning' ? PHASES.discovering : PHASES.comparing);
  const bar = h('div', { class: 'bar indeterminate' }, h('div', { class: 'fill' }));
  const count = h('p', { class: 'muted count' }, '');
  const cancelButton = h('button', { class: 'secondary', onclick: (e) => busy(e.currentTarget, () => call('cancel')) }, 'Cancel');
  mount(main, h('section', { class: 'card narrow center' },
    h('h1', {}, title), label, bar, count,
    h('p', { class: 'muted small' }, note),
    h('p', {}, cancelButton),
  ));

  const tick = async () => {
    let status;
    try {
      status = await call('scan_status');
    } catch (e) {
      showError(e);
      return setTimeout(tick, POLL_MS * 4);
    }
    if (status.state === 'running') {
      label.textContent = PHASES[status.phase] ?? 'Working…';
      const byBytes = status.phase === 'analyzing' && status.bytes_total > 0;
      const byFiles = (status.phase === 'analyzing' || status.phase === 'comparing') && status.total > 0;
      const known = byBytes || byFiles;
      bar.classList.toggle('indeterminate', !known);
      const fraction = byBytes ? status.bytes_done / status.bytes_total : status.done / (status.total || 1);
      bar.firstChild.style.setProperty('width', known ? `${Math.min(100, 100 * fraction)}%` : '');
      count.textContent = byBytes
        ? `${formatBytes(status.bytes_done)} of ${formatBytes(status.bytes_total)} · ${status.done.toLocaleString()} of ${status.total.toLocaleString()} files`
        : byFiles ? `${status.done.toLocaleString()} of ${status.total.toLocaleString()}` : '';
      return setTimeout(tick, POLL_MS);
    }
    if (status.state === 'failed') {
      toast(`${title} failed: ${status.error}`, 'error');
      return showFolders();
    }
    if (status.state === 'cancelled') {
      toast('Cancelled. Nothing was lost; files already read are remembered.');
      return showFolders();
    }
    state.lastSummary = status.state === 'done' ? status.summary : null;
    await loadGroups();
    showResults();
  };
  tick();
}

// ---------- screen: results ----------

async function loadGroups({ keepPrevious = false } = {}) {
  state.groups = await call('groups');
  state.selection.load(state.groups, { keepPrevious });
  state.shown = GROUPS_PER_PAGE;
}

function visibleGroups() {
  return state.filter === 'all' ? state.groups : state.groups.filter((g) => g.kind === state.filter);
}

function showResults() {
  main.replaceChildren();
  if (state.groups.length === 0) {
    mount(main, h('section', { class: 'card narrow center' },
      h('div', { class: 'big-emoji', 'aria-hidden': 'true' }, '🎉'),
      h('h1', {}, 'No duplicates found'),
      summaryLine(),
      h('button', { class: 'secondary', onclick: showFolders }, 'Back to folders'),
    ));
    return;
  }
  const bar = h('div', { class: 'action-bar', id: 'action-bar' });
  const list = h('div', { class: 'groups', id: 'groups' });
  mount(main, summaryBanner(), filterTabs(), bar, list);
  renderActionBar();
  renderGroupList();
}

function summaryLine() {
  const s = state.lastSummary;
  if (!s) return null;
  const parts = [`${plural(s.files_found, 'file')} scanned`];
  if (s.files_unchanged) parts.push(`${s.files_unchanged.toLocaleString()} unchanged since last time`);
  if (s.unreadable_count) parts.push(`${plural(s.unreadable_count, 'file')} couldn’t be read`);
  return h('p', { class: 'muted' }, `${parts.join(' · ')}.`, s.missing_folders.map((f) => h('span', { class: 'warn' }, ` Folder not found (skipped): ${f}.`)));
}

function summaryBanner() {
  const reclaim = state.groups.reduce((n, g) => n + g.reclaimable_bytes, 0);
  return h('section', { class: 'banner' },
    h('div', {},
      h('h1', {}, `${plural(state.groups.length, 'group')} of duplicates`),
      h('p', { class: 'lead' }, `Up to ${formatBytes(reclaim)} can be freed. The 👑 file in each group is the one we suggest keeping; click any file to switch it between keep and delete.`),
      summaryLine()),
    h('button', { class: 'secondary', onclick: showFolders }, 'Folders & rescan'),
  );
}

function filterTabs() {
  const count = (kind) => state.groups.filter((g) => g.kind === kind).length;
  const tabs = [
    ['all', `All (${state.groups.length})`],
    ['exact', `Exact copies (${count('exact')})`],
    ['near', `Look-alikes (${count('near')})`],
  ];
  return h('div', { class: 'tabs', role: 'tablist' }, tabs.map(([key, label]) =>
    h('button', {
      role: 'tab', class: state.filter === key ? 'tab active' : 'tab', 'aria-selected': String(state.filter === key),
      onclick: () => { state.filter = key; state.shown = GROUPS_PER_PAGE; showResults(); },
    }, label)));
}

function renderActionBar() {
  const bar = document.getElementById('action-bar');
  if (!bar) return;
  const marked = state.selection.markedFiles();
  const scope = state.filter === 'all' ? '' : ' (shown groups)';
  mount(bar,
    h('div', { class: 'tally' },
      h('strong', {}, plural(marked.length, 'file')), ' marked for deletion · ',
      h('strong', {}, formatBytes(state.selection.markedBytes()))),
    h('div', { class: 'bar-buttons' },
      h('button', { class: 'secondary', onclick: () => { state.selection.keepRecommended(visibleGroups()); refreshAll(); } }, `Keep recommended${scope}`),
      h('button', { class: 'secondary', onclick: () => { state.selection.clear(visibleGroups()); refreshAll(); } }, `Keep everything${scope}`),
      h('button', { class: 'danger', disabled: marked.length === 0, onclick: showReview }, `Review & delete (${marked.length})`)),
  );
}

function refreshAll() {
  renderActionBar();
  renderGroupList();
}

function renderGroupList() {
  const list = document.getElementById('groups');
  if (!list) return;
  const groups = visibleGroups();
  const page = groups.slice(0, state.shown);
  mount(list,
    page.map(groupCard),
    groups.length === 0 && h('p', { class: 'empty' }, 'No groups of this kind.'),
    groups.length > page.length && h('button', {
      class: 'secondary more',
      onclick: () => { state.shown += GROUPS_PER_PAGE; renderGroupList(); },
    }, `Show more (${groups.length - page.length} left)`),
  );
}

function groupCard(group) {
  const card = h('article', { class: 'group', dataset: { id: group.id } });
  fillGroupCard(card, group);
  return card;
}

function fillGroupCard(card, group) {
  const marked = state.selection.markedIn(group);
  const freed = group.members.filter((m) => marked.has(m.id)).reduce((n, m) => n + m.size, 0);
  const kind = group.kind === 'exact' ? 'Exact copies' : 'Look alike';
  mount(card,
    h('header', {},
      h('div', {},
        h('span', { class: `pill ${group.kind}` }, kind),
        group.tentative && h('span', { class: 'pill maybe', title: 'Found only because matching is set looser than Balanced. Nothing is pre-selected; check these before deleting.' }, 'Maybe'),
        h('span', { class: 'group-title' }, ` ${plural(group.members.length, 'file')} · deleting ${marked.size} frees ${formatBytes(freed)}`),
        h('p', { class: 'reason' }, `👑 ${group.reason}`)),
      h('div', { class: 'group-buttons' },
        h('button', { class: 'link', onclick: () => { state.selection.restore(group); updateGroup(card, group); } }, 'Restore'),
        h('button', { class: 'link', onclick: () => { state.selection.keepAll(group); updateGroup(card, group); } }, 'Keep all')),
    ),
    h('div', { class: 'tiles' }, group.members.map((file) => tile(card, group, file, marked.has(file.id)))),
  );
}

function updateGroup(card, group) {
  fillGroupCard(card, group);
  renderActionBar();
}

function tile(card, group, file, isMarked) {
  const recommended = file.id === group.recommended_id;
  const details = [dimensions(file), formatBytes(file.size)].filter(Boolean).join(' · ');
  const toggle = () => {
    if (!state.selection.toggle(group, file.id)) {
      toast('At least one copy in every group has to be kept.', 'warn');
      return;
    }
    updateGroup(card, group);
    card.querySelector(`[data-file="${file.id}"] .tile-main`)?.focus();
  };
  return h('div', { class: `tile ${isMarked ? 'marked' : 'kept'}`, dataset: { file: file.id } },
    h('button', {
      class: 'tile-main', onclick: toggle, 'aria-pressed': String(isMarked),
      title: `${file.path}\nClick to ${isMarked ? 'keep' : 'delete'} this file`,
    },
    h('div', { class: 'frame' },
      photo(file),
      recommended && h('span', { class: 'crown', title: 'Recommended to keep' }, '👑'),
      file.favorite && h('span', { class: 'fav', title: 'Rated 4★ or higher' }, '★'),
      h('span', { class: 'state' }, isMarked ? '🗑 Delete' : '✓ Keep')),
    h('div', { class: 'caption' },
      h('span', { class: 'name' }, file.name),
      h('span', { class: 'meta' }, details),
      h('span', { class: 'meta folder' }, file.folder))),
    h('button', { class: 'zoom', title: 'View larger', 'aria-label': `View ${file.name} larger`, onclick: () => viewLarge(group, file) }, '⤢'),
  );
}

function viewLarge(group, file) {
  const rows = [
    ['File', file.path],
    ['Size', formatBytes(file.size)],
    ['Dimensions', dimensions(file) ?? 'unknown'],
    ['Taken', file.taken_at ?? 'unknown'],
  ];
  if (file.id === group.recommended_id) rows.push(['Why keep it', group.reason]);
  else if (file.identical_to_recommended) rows.push(['Compared with 👑', 'Byte-for-byte identical']);
  else if (file.distance_to_recommended != null) rows.push(['Compared with 👑', 'Looks the same, but the file differs']);
  modal((close) => [
    h('div', { class: 'viewer' }, photo(file, { large: true, cls: 'large' })),
    h('dl', { class: 'facts' }, rows.map(([k, v]) => [h('dt', {}, k), h('dd', {}, v)])),
    h('div', { class: 'dialog-buttons' }, h('button', { class: 'secondary', onclick: () => close() }, 'Close')),
  ]);
}

// ---------- screen: review ----------

function showReview() {
  const marked = state.selection.markedFiles();
  if (marked.length === 0) return;
  const bytes = state.selection.markedBytes();
  const trash = state.info.trash_supported;
  let mode = trash ? 'trash' : 'permanent';

  const confirmBox = h('input', { type: 'checkbox', id: 'understand' });
  const confirmRow = h('label', { class: 'understand', for: 'understand' }, confirmBox, ' I understand these files can’t be recovered.');
  const go = h('button', { class: 'danger big' });
  const sync = () => {
    confirmRow.hidden = mode !== 'permanent';
    go.disabled = mode === 'permanent' && !confirmBox.checked;
    go.textContent = mode === 'trash'
      ? `Move ${plural(marked.length, 'file')} to the ${trashName()}`
      : `Permanently delete ${plural(marked.length, 'file')}`;
  };
  confirmBox.addEventListener('change', sync);
  const option = (value, title, text, disabled = false) => h('label', { class: `option ${disabled ? 'disabled' : ''}` },
    h('input', { type: 'radio', name: 'mode', value, checked: mode === value, disabled, onchange: () => { mode = value; sync(); } }),
    h('span', {}, h('strong', {}, title), h('br'), h('span', { class: 'muted' }, text)));

  const byGroup = new Map();
  for (const { group, file } of marked) {
    if (!byGroup.has(group)) byGroup.set(group, []);
    byGroup.get(group).push(file);
  }

  mount(main, h('section', { class: 'card review' },
    h('h1', {}, 'Review before deleting'),
    h('p', { class: 'lead' }, `${plural(marked.length, 'file')} from ${plural(byGroup.size, 'group')} will be removed, freeing ${formatBytes(bytes)}. In every group at least one copy stays.`),
    h('div', { class: 'options' },
      option('trash', `Move to the ${trashName()} (recommended)`, 'You can restore them from there if you change your mind.', !trash),
      option('permanent', 'Delete permanently', 'Frees the space immediately. This cannot be undone.')),
    confirmRow,
    h('div', { class: 'review-buttons' },
      h('button', { class: 'secondary', onclick: showResults }, '← Back to results'), go),
    h('ul', { class: 'review-list' }, [...byGroup].map(([group, files]) => {
      const kept = group.members.filter((m) => !files.includes(m));
      return h('li', {},
        h('div', { class: 'review-files' }, files.map((f) => h('div', { class: 'review-file' }, photo(f, { cls: 'mini' }),
          h('div', {}, h('span', { class: 'name' }, f.name), h('span', { class: 'meta' }, `${formatBytes(f.size)} · ${f.folder}`))))),
        h('p', { class: 'muted small' }, `Keeping: ${kept.map((k) => k.name).join(', ')}`));
    })),
  ));
  go.addEventListener('click', () => busy(go, () => doDelete(marked.map(({ file }) => file.id), mode === 'permanent')));
  sync();
}

function trashName() {
  return state.info.platform === 'windows' ? 'Recycle Bin' : 'Trash';
}

async function doDelete(ids, permanent) {
  const report = await call('delete', { ids, permanent });
  await loadGroups({ keepPrevious: true });
  const stats = await call('stats').catch(() => null);
  showDone(report, permanent, stats);
}

/** Puts the last delete action back from the Recycle Bin, then rescans so the files show again. */
async function undoLast(batch) {
  const r = await call('undo', { batch });
  const failed = r.failed.length ? ` ${plural(r.failed.length, 'file')} could not be restored: ${r.failed[0].reason}.` : '';
  toast(`Restored ${plural(r.restored, 'file')}.${failed}`, r.failed.length ? 'warn' : 'info');
  await startScan();
}

function showDone(report, permanent, stats) {
  const where = permanent ? 'deleted permanently' : `moved to the ${trashName()}`;
  const canUndo = !permanent && state.info.undo_supported && report.batch != null && report.deleted.length > 0;
  mount(main, h('section', { class: 'card narrow center' },
    h('div', { class: 'big-emoji', 'aria-hidden': 'true' }, report.failed.length ? '⚠️' : '✅'),
    h('h1', {}, `${plural(report.deleted.length, 'file')} ${where}`),
    h('p', { class: 'lead' }, `${formatBytes(report.bytes_freed)} freed.`),
    !permanent && h('p', { class: 'muted small' }, `Disk space returns when you empty the ${trashName()}. Until then you can restore these files.`),
    stats && stats.bytes_freed > 0 && h('p', { class: 'muted' }, `Freed so far with DupeLens: ${formatBytes(stats.bytes_freed)} (${plural(stats.files_deleted, 'file')}).`),
    report.failed.length > 0 && h('div', { class: 'failures' },
      h('p', {}, `${plural(report.failed.length, 'file was', 'files were')} skipped and left untouched:`),
      h('ul', {}, report.failed.map((f) => h('li', {}, h('span', { class: 'name' }, f.path || `File ${f.id}`), ` — ${f.reason}`)))),
    h('div', { class: 'review-buttons' },
      state.groups.length > 0 && h('button', { class: 'primary', onclick: showResults }, `Continue with ${plural(state.groups.length, 'remaining group')}`),
      canUndo && h('button', { class: 'secondary', onclick: (e) => busy(e.currentTarget, () => undoLast(report.batch)) }, `Undo: restore ${plural(report.deleted.length, 'file')}`),
      h('button', { class: 'secondary', onclick: showFolders }, 'Back to folders')),
  ));
}

// ---------- start-up ----------

async function start() {
  try {
    state.info = await call('info');
    document.getElementById('version').textContent = `v${state.info.version}`;
    const status = await call('scan_status');
    if (status.state === 'running') return showScan();
    state.lastSummary = status.state === 'done' ? status.summary : null;
  } catch (e) {
    if (e instanceof ApiError && e.kind === 'offline') {
      mount(main, h('section', { class: 'card narrow center' },
        h('h1', {}, 'Open DupeLens as the app'),
        h('p', { class: 'lead' }, e.message)));
      return;
    }
    showError(e);
  }
  await showFolders();
  // Offer results from an earlier session without making the user rescan.
  try {
    await loadGroups();
    if (main.querySelector('.scan-row')) renderFolders();
  } catch {
    // Not available yet (e.g. never scanned); the folder screen already works.
  }
}

start();
