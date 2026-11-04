// DupeLens UI: Overview -> Scanning -> Results (thumbnails + live preview) -> Review -> Done,
// plus Settings. Talks to the backend only through api.js, which forwards to the Tauri app.
// No browser popups (alert/confirm/prompt): some Tauri webviews don't support them, so
// dialogs are drawn in the page. Design: docs/UI.md ("Visual design").

import { ApiError, call, pickFolder, thumbUrl } from './api.js';
import { h, mount } from './dom.js';
import { byCategory, categoryName, dimensions, formatBytes, nextMilestone, plural } from './format.js';
import { Selection } from './selection.js';

const main = document.getElementById('main');
const GROUPS_PER_PAGE = 30;
const POLL_MS = 300;
const HOVER_MS = 70;

const state = {
  info: { trash_supported: true, undo_supported: false, platform: '' },
  screen: 'overview', // 'overview' | 'scan' | 'results' | 'review' | 'done' | 'settings'
  folders: [],
  groups: [],
  selection: new Selection(),
  filter: 'all', // 'all' or a file type
  kind: 'all', // 'all' | 'exact' | 'near' | 'maybe'
  shown: GROUPS_PER_PAGE,
  lastSummary: null,
  settings: null,
  pendingPreset: null, // chosen on the Overview but not applied yet
  suggested: [],
  stats: null,
  active: null, // { gid, fid }: the copy shown in the preview pane
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

function preview(file, { large = false, cls = 'thumb' } = {}) {
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

/** Drops results held by the page, for when the engine has dropped its own. */
function forgetResults() {
  state.groups = [];
  state.selection.load([]);
  state.lastSummary = null;
  state.active = null;
}

// ---------- top bar ----------

function go(screen) {
  if (screen === 'overview') return showOverview();
  if (screen === 'results') return showResults();
  if (screen === 'settings') return showSettings();
}

function renderNav() {
  const item = (key, label, disabled = false) =>
    h('button', {
      class: state.screen === key ? 'active' : '', disabled: disabled || state.screen === 'scan',
      'aria-current': state.screen === key ? 'page' : false, onclick: () => go(key),
    }, label);
  mount(document.getElementById('nav'),
    item('overview', 'Overview'),
    item('results', 'Results', state.groups.length === 0),
    item('settings', 'Settings'));
}

function enter(screen) {
  const changed = state.screen !== screen;
  state.screen = screen;
  renderNav();
  if (changed) {
    window.scrollTo(0, 0);
    main.focus({ preventScroll: true });
  }
}

// ---------- screen: overview ----------

async function showOverview() {
  enter('overview');
  mount(main, h('p', { class: 'muted' }, 'Loading…'));
  try {
    state.folders = await call('folders');
    state.settings = await call('get_settings');
    state.suggested = await call('suggested_folders').catch(() => []);
    state.stats = await call('stats').catch(() => null);
  } catch (e) {
    showError(e);
  }
  renderOverview();
}

function renderOverview() {
  if (state.screen !== 'overview') return;
  const hasResults = state.groups.length > 0;
  mount(main, h('div', { class: 'overview' },
    h('div', { class: 'stack' }, hasResults ? reclaimPanel() : introPanel(), foldersPanel()),
    h('div', { class: 'stack' }, matchingPanel())));
}

function introPanel() {
  const s = state.lastSummary;
  return h('section', { class: 'panel' },
    h('h1', {}, s ? 'No duplicates found' : 'Find duplicate files'),
    h('p', { class: 'lead' }, s
      ? `${plural(s.files_found, 'file')} checked. Nothing in these folders matches at the current setting.`
      : 'Choose the folders to look in. DupeLens only reads them, works entirely offline, and never deletes anything until you confirm.'),
    state.stats && state.stats.bytes_freed > 0 && h('p', { class: 'muted small' },
      `Freed so far with DupeLens: `, h('span', { class: 'num' }, formatBytes(state.stats.bytes_freed))));
}

function reclaimPanel() {
  const rows = byCategory(state.groups);
  const total = rows.reduce((n, r) => n + r.bytes, 0);
  const files = rows.reduce((n, r) => n + r.files, 0);
  return h('section', { class: 'panel' },
    h('h2', {}, 'Space you can reclaim'),
    h('div', { class: 'hero-number' }, formatBytes(total)),
    h('p', { class: 'muted' }, `${plural(state.groups.length, 'group')} of duplicates, ${plural(files, 'file')} in all. Nothing is deleted until you review it.`),
    h('div', { class: 'typebar', role: 'img', 'aria-label': 'Reclaimable space by file type' },
      rows.map((r) => h('span', { class: `t-${r.key}`, title: `${categoryName(r.key)}: ${formatBytes(r.bytes)}`, dataset: { grow: String(r.bytes) } }))),
    h('table', { class: 'types' },
      h('thead', {}, h('tr', {}, h('th', {}, 'Type'), h('th', { class: 'r' }, 'Groups'), h('th', { class: 'r' }, 'Files'), h('th', { class: 'r' }, 'Reclaimable'))),
      h('tbody', {}, rows.map((r) => h('tr', {},
        h('td', {}, h('span', { class: `swatch t-${r.key}` }), categoryName(r.key)),
        h('td', { class: 'r num' }, r.groups.toLocaleString()),
        h('td', { class: 'r num' }, r.files.toLocaleString()),
        h('td', { class: 'r num' }, formatBytes(r.bytes)))))),
    h('p', { class: 'row' }, h('button', { class: 'primary', onclick: showResults }, 'Review results')),
  );
}

/** Sizes the stacked bar: each segment's width is its share of the total, set from script. */
function sizeTypeBar() {
  const segments = [...main.querySelectorAll('.typebar > span')];
  const total = segments.reduce((n, s) => n + Number(s.dataset.grow), 0) || 1;
  for (const s of segments) s.style.setProperty('flex', `${Number(s.dataset.grow) / total} 1 0`);
}

function foldersPanel() {
  const pathInput = h('input', { type: 'text', placeholder: 'Or paste a folder path', 'aria-label': 'Folder path', spellcheck: 'false' });
  const addTyped = async (button) => {
    if (!pathInput.value.trim()) return pathInput.focus();
    await busy(button, async () => {
      await addFolder(pathInput.value);
      pathInput.value = '';
    });
  };
  const addButton = h('button', { onclick: (e) => addTyped(e.currentTarget) }, 'Add');
  pathInput.addEventListener('keydown', (e) => e.key === 'Enter' && addTyped(addButton));

  const chosen = new Set(state.folders.map((f) => f.toLowerCase()));
  const left = state.suggested.filter((s) => !chosen.has(s.path.toLowerCase()));
  return h('section', { class: 'panel' },
    h('h2', {}, 'Folders'),
    state.folders.length === 0
      ? h('p', { class: 'muted' }, 'No folders yet. Pick the places where duplicates tend to build up.')
      : h('ul', { class: 'folder-list' }, state.folders.map((f) => h('li', {},
        h('span', { class: 'folder-path', title: f }, f),
        h('button', { class: 'icon', title: 'Remove this folder', 'aria-label': `Remove ${f}`, onclick: (e) => busy(e.currentTarget, () => removeFolder(f)) }, 'Remove')))),
    left.length > 0 && state.folders.length === 0 && h('div', { class: 'chips' }, left.map((s) =>
      h('button', { class: 'chip', title: s.path, onclick: (e) => busy(e.currentTarget, () => addFolder(s.path)) }, `+ ${s.name}`))),
    h('div', { class: 'row' }, h('button', { onclick: (e) => busy(e.currentTarget, chooseFolder) }, 'Choose folder…')),
    h('div', { class: 'path-row' }, pathInput, addButton));
}

function matchingPanel() {
  const st = state.settings;
  const hasResults = state.groups.length > 0 || state.lastSummary != null;
  const chosen = state.pendingPreset ?? st?.preset;
  const changed = state.pendingPreset != null && state.pendingPreset !== st?.preset;
  const presets = st ? st.presets.filter((p) => p.key !== 'custom' || st.preset === 'custom') : [];
  const rerun = h('button', { class: changed ? 'primary' : '', disabled: !changed, onclick: (e) => busy(e.currentTarget, rerunMatching) }, 'Re-run');
  const scan = h('button', { class: hasResults ? '' : 'primary big', disabled: state.folders.length === 0, onclick: (e) => busy(e.currentTarget, startScan) },
    hasResults ? 'Scan folders again' : 'Scan for duplicates');
  return h('section', { class: 'panel' },
    h('h2', {}, 'What counts as a duplicate'),
    st && h('div', { class: 'preset-list', role: 'radiogroup', 'aria-label': 'How alike must files be' }, presets.map((p) =>
      h('label', { class: 'preset' },
        h('input', {
          type: 'radio', name: 'preset', value: p.key, checked: chosen === p.key,
          onchange: () => {
            state.pendingPreset = p.key === st.preset ? null : p.key;
            renderOverview();
            document.querySelector(`input[name="preset"][value="${p.key}"]`)?.focus();
          },
        }),
        h('span', {}, h('strong', {}, p.name), h('span', { class: 'muted' }, p.description))))),
    changed && h('p', { class: 'pending-note' }, hasResults ? 'Re-run to update your results. Folders are not read again.' : 'Applies to your next scan.'),
    h('div', { class: 'row' }, hasResults && rerun, scan),
    h('p', { class: 'muted small' }, h('button', { class: 'link', onclick: showSettings }, 'Advanced values…')),
  );
}

/** Applies a preset chosen on the Overview, then regroups the last scan without reading folders. */
async function rerunMatching() {
  await applyPendingPreset();
  await startRegroup();
}

async function applyPendingPreset() {
  const preset = state.pendingPreset;
  if (!preset) return;
  try {
    state.settings = await call('set_settings', { preset });
  } catch (e) {
    state.pendingPreset = null;
    state.settings = await call('get_settings').catch(() => state.settings);
    renderOverview();
    throw e;
  }
  state.pendingPreset = null;
  forgetResults(); // the engine drops its cached results when settings change
  renderNav();
}

async function chooseFolder() {
  const path = await pickFolder();
  if (path) await addFolder(path);
}

async function addFolder(path) {
  const stored = await call('add_folder', { path });
  state.folders = await call('folders');
  renderOverview();
  toast(`Added ${stored}`);
}

async function removeFolder(path) {
  await call('remove_folder', { path });
  state.folders = await call('folders');
  renderOverview();
}

// ---------- screen: scanning ----------

const STEPS = [
  ['discovering', 'Finding files'],
  ['analyzing', 'Reading files'],
  ['saving', 'Saving results'],
  ['comparing', 'Comparing look-alikes'],
];

async function startScan() {
  await applyPendingPreset();
  await call('start_scan');
  showScan({ steps: STEPS, title: 'Scanning' });
}

/** Re-groups the last scan's results with the current matching setting; no folders are re-read. */
async function startRegroup() {
  await call('start_regroup');
  showScan({ steps: STEPS.slice(2), title: 'Updating results', note: 'Using what was already scanned, so only look-alike comparisons that are new take time.' });
}

function showScan({ steps = STEPS, title = 'Scanning', note = 'Files that haven’t changed since the last scan are skipped, so rescans are quick.' } = {}) {
  enter('scan');
  const percent = h('div', { class: 'percent num' }, '…');
  const bar = h('div', { class: 'bar indeterminate', role: 'progressbar', 'aria-label': title, 'aria-valuemin': '0', 'aria-valuemax': '100' }, h('div', { class: 'fill' }));
  const counts = h('p', { class: 'counts' }, '');
  const list = h('ol', { class: 'steps' }, steps.map(([, label]) => h('li', {}, label)));
  const cancelButton = h('button', { onclick: (e) => busy(e.currentTarget, () => call('cancel')) }, 'Cancel');
  mount(main, h('section', { class: 'panel narrow' },
    h('h2', {}, title), percent, bar, counts, list,
    h('p', { class: 'muted small' }, note),
    h('p', {}, cancelButton)));

  const tick = async () => {
    if (state.screen !== 'scan') return;
    let status;
    try {
      status = await call('scan_status');
    } catch (e) {
      showError(e);
      return setTimeout(tick, POLL_MS * 4);
    }
    if (status.state === 'running') {
      const at = steps.findIndex(([key]) => key === status.phase);
      [...list.children].forEach((li, i) => { li.className = i === at ? 'now' : i < at ? 'past' : ''; });
      const byBytes = status.phase === 'analyzing' && status.bytes_total > 0;
      const byCount = (status.phase === 'analyzing' || status.phase === 'comparing') && status.total > 0;
      const known = byBytes || byCount;
      const fraction = byBytes ? status.bytes_done / status.bytes_total : status.done / (status.total || 1);
      bar.classList.toggle('indeterminate', !known);
      bar.firstChild.style.setProperty('width', known ? `${Math.min(100, 100 * fraction)}%` : '');
      percent.textContent = known ? `${Math.min(100, Math.floor(100 * fraction))}%` : '…';
      if (known) bar.setAttribute('aria-valuenow', String(Math.min(100, Math.floor(100 * fraction))));
      else bar.removeAttribute('aria-valuenow');
      counts.textContent = byBytes
        ? `${formatBytes(status.bytes_done)} of ${formatBytes(status.bytes_total)} · ${status.done.toLocaleString()} of ${status.total.toLocaleString()} files`
        : byCount ? `${status.done.toLocaleString()} of ${status.total.toLocaleString()} pairs` : '';
      return setTimeout(tick, POLL_MS);
    }
    if (status.state === 'failed' || status.state === 'cancelled') {
      // The engine has dropped its cached results (and a regroup may already have saved a new
      // matching setting), so the groups held here no longer match it. Forget them: the person
      // rescans or updates the results, rather than acting on a list from the old setting.
      forgetResults();
      if (status.state === 'failed') toast(`${title} failed: ${status.error}`, 'error');
      else toast('Cancelled. Nothing was lost; files already read are remembered.');
      return showOverview();
    }
    state.lastSummary = status.state === 'done' ? status.summary : null;
    try {
      await loadGroups();
    } catch (e) {
      forgetResults();
      showError(e);
      return showOverview();
    }
    state.filter = 'all';
    state.kind = 'all';
    if (state.groups.length === 0) return showOverview();
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
  return state.groups.filter((g) =>
    (state.filter === 'all' || g.category === state.filter)
    && (state.kind === 'all' || (state.kind === 'maybe' ? g.tentative : g.kind === state.kind && !g.tentative)));
}

function shownGroups() {
  return visibleGroups().slice(0, state.shown);
}

function showResults() {
  enter('results');
  if (state.groups.length === 0) {
    mount(main, h('section', { class: 'panel narrow center' },
      h('h1', {}, 'No duplicates to review'),
      h('p', { class: 'lead' }, 'Scan your folders to find some.'),
      h('button', { onclick: showOverview }, 'Back to overview')));
    return;
  }
  if (state.filter !== 'all' && !state.groups.some((g) => g.category === state.filter)) state.filter = 'all';
  if (!findActive()) setDefaultActive();
  const bar = h('div', { class: 'toolbar', id: 'toolbar' });
  mount(main, bar,
    h('div', { class: 'results' },
      h('div', { class: 'groups', id: 'groups' }),
      h('aside', { class: 'pane', id: 'pane', 'aria-label': 'Preview' })));
  renderToolbar();
  renderGroupList();
  renderPane();
}

/** The active copy, only if its group is one the list is showing right now. */
function findActive() {
  const { gid, fid } = state.active ?? {};
  const group = shownGroups().find((g) => g.id === gid);
  const file = group?.members.find((m) => m.id === fid);
  return group && file ? { group, file } : null;
}

function setDefaultActive() {
  const g = shownGroups()[0];
  state.active = g ? { gid: g.id, fid: g.recommended_id } : null;
}

function renderToolbar() {
  const bar = document.getElementById('toolbar');
  if (!bar) return;
  const marked = state.selection.markedFiles();
  const counts = new Map();
  for (const g of state.groups) counts.set(g.category, (counts.get(g.category) ?? 0) + 1);
  const chip = (key, label, n) => h('button', {
    class: state.filter === key ? 'chip active' : 'chip', 'aria-pressed': String(state.filter === key),
    dataset: { chip: key },
    onclick: () => { state.filter = key; state.shown = GROUPS_PER_PAGE; showResults(); document.querySelector(`[data-chip="${key}"]`)?.focus(); },
  }, label, h('span', { class: 'num' }, n.toLocaleString()));
  const kind = h('select', {
    'aria-label': 'Kind of match',
    onchange: (e) => { state.kind = e.currentTarget.value; state.shown = GROUPS_PER_PAGE; showResults(); document.querySelector('.toolbar select')?.focus(); },
  }, [['all', 'All matches'], ['exact', 'Exact copies'], ['near', 'Look-alikes'], ['maybe', 'Maybe']]
    .map(([v, label]) => h('option', { value: v, selected: state.kind === v }, label)));
  const scope = state.filter === 'all' && state.kind === 'all' ? '' : ' (shown)';
  mount(bar,
    h('div', { class: 'toolbar-row' },
      h('div', { class: 'chips' },
        chip('all', 'All', state.groups.length),
        [...counts].map(([key, n]) => chip(key, categoryName(key), n))),
      kind,
      h('div', { class: 'spacer' }),
      h('button', { class: 'link', title: 'Change on the Overview', onclick: showOverview }, `Matching: ${presetName()}`)),
    h('div', { class: 'toolbar-row' },
    h('div', { class: 'tally' }, h('span', { class: 'num' }, plural(marked.length, 'file')), ' to delete · ', h('span', { class: 'num' }, formatBytes(state.selection.markedBytes()))),
    h('div', { class: 'spacer' }),
    h('button', { class: 'link', onclick: () => { state.selection.keepRecommended(visibleGroups()); refreshAll(); } }, `Keep recommended${scope}`),
    h('button', { class: 'link', onclick: () => { state.selection.clear(visibleGroups()); refreshAll(); } }, `Keep everything${scope}`),
    h('button', { class: 'danger', disabled: marked.length === 0, onclick: showReview }, `Review and delete (${marked.length})`)));
}

function presetName() {
  const st = state.settings;
  return st?.presets.find((p) => p.key === st.preset)?.name ?? 'Balanced';
}

function refreshAll() {
  renderToolbar();
  renderGroupList();
  renderPane();
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
      class: 'more',
      onclick: () => { state.shown += GROUPS_PER_PAGE; renderGroupList(); },
    }, `Show more (${groups.length - page.length} left)`));
}

function groupCard(group) {
  const card = h('article', { class: 'group', dataset: { id: group.id } });
  fillGroupCard(card, group);
  return card;
}

function fillGroupCard(card, group) {
  const marked = state.selection.markedIn(group);
  const freed = group.members.filter((m) => marked.has(m.id)).reduce((n, m) => n + m.size, 0);
  mount(card,
    h('header', {},
      h('div', { class: 'group-title' },
        h('span', { class: `pill ${group.kind}` }, group.kind === 'exact' ? 'Exact' : 'Look-alike'),
        group.tentative && h('span', { class: 'pill maybe', title: 'Found only because matching is set looser than Balanced. Nothing is pre-selected; check these before deleting.' }, 'Maybe'),
        h('span', { class: 'num' }, `${categoryName(group.category)} · ${plural(group.members.length, 'file')} · deleting ${marked.size} frees ${formatBytes(freed)}`)),
      h('div', { class: 'group-buttons' },
        h('button', { class: 'link', onclick: () => { state.selection.restore(group); updateGroup(card, group); } }, 'Restore'),
        h('button', { class: 'link', onclick: () => { state.selection.keepAll(group); updateGroup(card, group); } }, 'Keep all'))),
    h('p', { class: 'reason' }, `Suggested copy: ${group.reason}`),
    h('div', { class: 'tiles' }, group.members.map((file) => tile(card, group, file, marked.has(file.id)))));
}

function updateGroup(card, group) {
  // Redrawing replaces the buttons, so note which one had focus and put focus back on its twin.
  const was = document.activeElement;
  const tileId = was?.closest?.('.tile')?.dataset.file;
  const headerText = was?.closest?.('.group-buttons') ? was.textContent : null;
  fillGroupCard(card, group);
  renderToolbar();
  renderPane();
  if (tileId) card.querySelector(`.tile[data-file="${tileId}"] .tile-main`)?.focus({ preventScroll: true });
  else if (headerText) [...card.querySelectorAll('.group-buttons button')].find((b) => b.textContent === headerText)?.focus({ preventScroll: true });
}

function tile(card, group, file, isMarked) {
  const recommended = file.id === group.recommended_id;
  const active = state.active?.gid === group.id && state.active?.fid === file.id;
  const details = [dimensions(file), formatBytes(file.size)].filter(Boolean).join(' · ');
  let hoverTimer;
  const toggle = () => {
    if (!state.selection.toggle(group, file.id)) {
      toast('At least one copy in every group has to be kept.', 'warn');
      return;
    }
    state.active = { gid: group.id, fid: file.id };
    updateGroup(card, group);
    card.querySelector(`[data-file="${file.id}"] .tile-main`)?.focus();
  };
  const el = h('div', {
    class: `tile ${isMarked ? 'marked' : 'kept'}${active ? ' active' : ''}`, dataset: { file: file.id },
    onmouseenter: () => { clearTimeout(hoverTimer); hoverTimer = setTimeout(() => setActive(group, file), HOVER_MS); },
    onmouseleave: () => clearTimeout(hoverTimer),
    onfocusin: () => setActive(group, file),
  },
  h('button', {
    class: 'tile-main', onclick: toggle, 'aria-pressed': String(isMarked),
    title: `${file.path}\nClick to ${isMarked ? 'keep' : 'delete'} this file`,
  },
  h('div', { class: 'frame' },
    preview(file),
    h('span', { class: 'tag' }, isMarked ? 'Delete' : recommended ? 'Keep · suggested' : 'Keep')),
  h('div', { class: 'caption' },
    h('span', { class: 'name', title: file.name }, file.name),
    h('span', { class: 'meta' }, details),
    h('span', { class: 'meta folder', title: file.folder }, file.folder))),
  h('button', { class: 'zoom', title: 'View full size (Space)', 'aria-label': `View ${file.name} full size`, onclick: () => viewLarge(group, file) }, '+'));
  return el;
}

/** Points the preview pane at a copy without redrawing the list. */
function setActive(group, file) {
  if (state.active?.gid === group.id && state.active?.fid === file.id) return;
  state.active = { gid: group.id, fid: file.id };
  document.querySelectorAll('.tile.active').forEach((t) => t.classList.remove('active'));
  document.querySelector(`.group[data-id="${group.id}"] .tile[data-file="${file.id}"]`)?.classList.add('active');
  renderPane();
}

function renderPane() {
  const pane = document.getElementById('pane');
  if (!pane) return;
  const found = findActive();
  if (!found) {
    mount(pane, h('div', { class: 'pane-body' }, h('p', { class: 'muted' }, 'Point at a file to see it here.')));
    return;
  }
  const { group, file } = found;
  const isMarked = state.selection.isMarked(group, file.id);
  const hadFocus = pane.contains(document.activeElement) ? document.activeElement.textContent : null;
  mount(pane,
    h('div', { class: 'pane-image' }, preview(file, { large: true, cls: 'large' })),
    h('div', { class: 'pane-body' },
      h('h3', {}, file.name),
      h('div', { class: 'pane-path' }, file.path),
      h('dl', { class: 'facts' }, factRows(group, file).map(([k, v]) => [h('dt', {}, k), h('dd', {}, v)])),
      h('div', { class: 'row' },
        h('button', { 'aria-pressed': String(!isMarked), class: isMarked ? '' : 'primary', onclick: () => setMarked(group, file, false) }, 'Keep'),
        h('button', { 'aria-pressed': String(isMarked), class: isMarked ? 'danger' : '', onclick: () => setMarked(group, file, true) }, 'Delete')),
      h('p', { class: 'keys' },
        h('kbd', {}, 'Space'), ' full size  ', h('kbd', {}, '←'), h('kbd', {}, '→'), ' move  ',
        h('kbd', {}, 'K'), ' keep  ', h('kbd', {}, 'D'), ' delete')));
  if (hadFocus) [...pane.querySelectorAll('.row button')].find((b) => b.textContent === hadFocus)?.focus({ preventScroll: true });
}

function factRows(group, file) {
  const rows = [
    ['Size', formatBytes(file.size)],
    ['Type', categoryName(file.category)],
    ['Dimensions', dimensions(file) ?? 'n/a'],
    ['Modified', file.modified_secs ? new Date(file.modified_secs * 1000).toLocaleString() : 'unknown'],
  ];
  if (file.taken_at) rows.push(['Taken', file.taken_at]);
  if (file.id === group.recommended_id) rows.push(['Suggested', group.reason]);
  else if (file.identical_to_recommended) rows.push(['Compared', 'Identical to the suggested copy, byte for byte']);
  else if (file.distance_to_recommended != null) rows.push(['Compared', 'Looks the same as the suggested copy, but the file differs']);
  return rows;
}

function setMarked(group, file, wantMarked) {
  if (state.selection.isMarked(group, file.id) === wantMarked) return;
  if (!state.selection.toggle(group, file.id)) {
    toast('At least one copy in every group has to be kept.', 'warn');
    return;
  }
  const card = document.querySelector(`.group[data-id="${group.id}"]`);
  if (card) updateGroup(card, group);
  else refreshAll();
}

function viewLarge(group, file) {
  modal((close) => [
    h('div', { class: 'viewer' }, preview(file, { large: true, cls: 'large' })),
    h('dl', { class: 'facts' }, [['File', file.path], ...factRows(group, file)].map(([k, v]) => [h('dt', {}, k), h('dd', {}, v)])),
    h('div', { class: 'dialog-buttons' }, h('button', { onclick: () => close() }, 'Close')),
  ]);
}

/** Keyboard: arrows move between copies, K keeps, D deletes, Space shows full size. */
function onKeys(e) {
  if (state.screen !== 'results' || e.ctrlKey || e.metaKey || e.altKey) return;
  if (document.querySelector('.backdrop')) return;
  const t = e.target;
  if (t instanceof HTMLElement && t.matches('input, select, textarea')) return;
  const flat = shownGroups().flatMap((g) => g.members.map((m) => ({ group: g, file: m })));
  const at = flat.findIndex((x) => x.group.id === state.active?.gid && x.file.id === state.active?.fid);
  const found = findActive();
  const move = (to) => {
    const target = flat[to];
    if (!target) return;
    setActive(target.group, target.file);
    const main = document.querySelector(`.tile[data-file="${target.file.id}"] .tile-main`);
    main?.focus({ preventScroll: true });
    main?.scrollIntoView({ block: 'nearest' });
  };
  const jumpGroup = (delta) => {
    const groups = shownGroups();
    const gi = groups.findIndex((g) => g.id === state.active?.gid) + delta;
    const g = groups[gi];
    if (g) move(flat.findIndex((x) => x.group.id === g.id));
  };
  if (e.key === ' ' && spaceBelongsToButton(t)) return;
  switch (e.key) {
    case 'ArrowRight': move(at + 1); break;
    case 'ArrowLeft': move(at - 1); break;
    case 'ArrowDown': jumpGroup(1); break;
    case 'ArrowUp': jumpGroup(-1); break;
    case 'k': case 'K': if (found) setMarked(found.group, found.file, false); break;
    case 'd': case 'D': if (found) setMarked(found.group, found.file, true); break;
    case ' ': if (found) viewLarge(found.group, found.file); break;
    default: return;
  }
  e.preventDefault();
}
/** A focused button (other than a photo tile) keeps its own Space behaviour. */
function spaceBelongsToButton(t) {
  return t instanceof HTMLElement && !!t.closest('button') && !t.closest('.tile-main');
}
document.addEventListener('keydown', onKeys);
// A button also activates on Space's key-up; stop that so Space only opens the viewer.
document.addEventListener('keyup', (e) => {
  if (e.key === ' ' && state.screen === 'results' && !(e.target instanceof HTMLElement && e.target.matches('input, select, textarea')) && !spaceBelongsToButton(e.target)) e.preventDefault();
});

// ---------- screen: review ----------

function showReview() {
  const marked = state.selection.markedFiles();
  if (marked.length === 0) return;
  enter('review');
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
  const notPhotos = marked.filter(({ file }) => file.category !== 'photo');
  const maybes = marked.filter(({ group }) => group.tentative);

  mount(main, h('section', { class: 'review' },
    h('h1', {}, 'Review before deleting'),
    h('p', { class: 'lead' }, `${plural(marked.length, 'file')} from ${plural(byGroup.size, 'group')} will be removed, freeing `, h('span', { class: 'num' }, formatBytes(bytes)), '. In every group at least one copy stays.'),
    notPhotos.length > 0 && h('div', { class: 'notice' }, `${plural(notPhotos.length, 'file')} here ${notPhotos.length === 1 ? 'is' : 'are'} not ${notPhotos.length === 1 ? 'a photo' : 'photos'} (${[...new Set(notPhotos.map(({ file }) => categoryName(file.category).toLowerCase()))].join(', ')}). They match another file byte for byte, but check you do not need the extra copy.`),
    maybes.length > 0 && h('div', { class: 'notice' }, `${plural(maybes.length, 'file')} come from “Maybe” groups: look-alikes that only a looser matching setting finds. You chose these yourself, so check each one.`),
    h('ul', { class: 'promises' },
      h('li', {}, 'Nothing is deleted until you press the button below.'),
      h('li', {}, 'At least one copy in every group always stays.'),
      h('li', {}, trash ? `The ${trashName()} is the default, so you can restore these files.` : 'This computer has no Recycle Bin or Trash, so deletion is permanent.')),
    h('div', { class: 'options' },
      option('trash', `Move to the ${trashName()} (recommended)`, `You can restore them from there. Disk space returns when you empty the ${trashName()}.`, !trash),
      option('permanent', 'Delete permanently', 'Frees the space immediately. This cannot be undone.')),
    confirmRow,
    h('div', { class: 'review-buttons spread' }, h('button', { onclick: showResults }, '← Back to results'), go),
    h('ul', { class: 'review-list' }, [...byGroup].map(([group, files]) => {
      const kept = group.members.filter((m) => !files.includes(m));
      const row = (f) => h('div', { class: 'review-file' }, preview(f, { cls: 'mini' }),
        h('div', {}, h('span', { class: 'name' }, f.name), h('span', { class: 'meta' }, `${formatBytes(f.size)} · ${f.folder}`)));
      return h('li', {},
        h('div', { class: 'review-col' }, h('h2', {}, 'Will be removed'), files.map(row)),
        h('div', { class: 'review-col' }, h('h2', {}, 'Stays'), kept.map(row)));
    }))));
  go.addEventListener('click', () => busy(go, () => doDelete(marked, mode === 'permanent')));
  sync();
}

function trashName() {
  return state.info.platform === 'windows' ? 'Recycle Bin' : 'Trash';
}

async function doDelete(marked, permanent) {
  const byFile = new Map(marked.map(({ file }) => [file.id, file]));
  const report = await call('delete', { ids: [...byFile.keys()], permanent });
  const freedByType = byCategory(report.deleted.filter((id) => byFile.has(id)).map((id) => {
    const f = byFile.get(id);
    return { category: f.category, members: [f], reclaimable_bytes: f.size };
  }));
  try {
    await loadGroups({ keepPrevious: true });
  } catch (e) {
    forgetResults();
    showError(e);
  }
  const stats = await call('stats').catch(() => null);
  showDone(report, permanent, stats, freedByType);
}

/** Puts the last delete action back from the Recycle Bin, then rescans so the files show again. */
async function undoLast(batch) {
  const r = await call('undo', { batch });
  const failed = r.failed.length ? ` ${plural(r.failed.length, 'file')} could not be restored: ${r.failed[0].reason}.` : '';
  toast(`Restored ${plural(r.restored, 'file')}.${failed}`, r.failed.length ? 'warn' : 'info');
  try {
    await startScan();
  } catch (e) {
    showError(e);
    forgetResults();
    await showOverview();
  }
}

// ---------- screen: done ----------

function showDone(report, permanent, stats, freedByType) {
  enter('done');
  const where = permanent ? 'deleted permanently' : `moved to the ${trashName()}`;
  const canUndo = !permanent && state.info.undo_supported && report.batch != null && report.deleted.length > 0;
  mount(main, h('section', { class: 'panel narrow center' },
    h('h2', {}, `${plural(report.deleted.length, 'file')} ${where}`),
    h('div', { class: 'freed' }, formatBytes(report.bytes_freed)),
    h('p', { class: 'muted' }, permanent ? 'Freed.' : `Disk space returns when you empty the ${trashName()}. Until then you can restore these files.`),
    freedByType.length > 1 && h('p', { class: 'muted small' }, freedByType.map((r) => `${categoryName(r.key)} ${formatBytes(r.bytes)}`).join(' · ')),
    report.failed.length > 0 && h('div', { class: 'failures' },
      h('p', {}, `${plural(report.failed.length, 'file was', 'files were')} skipped and left untouched:`),
      h('ul', {}, report.failed.map((f) => h('li', {}, h('span', { class: 'name' }, f.path || `File ${f.id}`), ` — ${f.reason}`)))),
    h('div', { class: 'review-buttons' },
      state.groups.length > 0 && h('button', { class: 'primary', onclick: showResults }, `Continue with ${plural(state.groups.length, 'remaining group')}`),
      canUndo && h('button', { onclick: (e) => busy(e.currentTarget, () => undoLast(report.batch)) }, `Undo: restore ${plural(report.deleted.length, 'file')}`),
      h('button', { onclick: showOverview }, 'Back to overview')),
    milestonePanel(stats)));
}

/** A quiet lifetime total and the next round number. Stored only on this computer. */
function milestonePanel(stats) {
  if (!stats || stats.bytes_freed <= 0) return null;
  const next = nextMilestone(stats.bytes_freed);
  const bar = h('div', { class: 'bar' }, h('div', { class: 'fill' }));
  if (next) bar.firstChild.style.setProperty('width', `${Math.min(100, (100 * stats.bytes_freed) / next)}%`);
  return h('div', { class: 'milestone' },
    h('h2', {}, 'Freed so far with DupeLens'),
    h('p', {}, h('span', { class: 'num' }, formatBytes(stats.bytes_freed)), ` across ${plural(stats.files_deleted, 'file')}.`),
    next && [bar, h('p', { class: 'muted small' }, `Next milestone: ${formatBytes(next)}. Kept on this computer only.`)]);
}

// ---------- screen: settings ----------

// Limits the engine accepts for the Custom preset (docs/decisions/0014).
const CUSTOM_FIELDS = [
  { key: 'phash_max', label: 'Overall look', hint: 'How many of 64 bits of the picture’s overall pattern may differ. Lower is stricter.', min: 0, max: 24, step: 1 },
  { key: 'dhash_max', label: 'Fine detail', hint: 'How many of 128 bits of edge detail may differ. Lower is stricter.', min: 0, max: 32, step: 1 },
  { key: 'aspect_tolerance', label: 'Shape tolerance (%)', hint: 'How much the width-to-height ratio may differ, so a crop still matches.', min: 0, max: 25, step: 0.5, percent: true },
  { key: 'max_local_difference', label: 'Pixel change', hint: 'How much any one area may change when the pictures are compared closely. Lower is stricter.', min: 0, max: 60, step: 1 },
];

async function showSettings() {
  enter('settings');
  mount(main, h('p', { class: 'muted' }, 'Loading…'));
  try {
    state.settings = await call('get_settings');
  } catch (e) {
    showError(e);
  }
  renderSettings();
}

function renderSettings() {
  const st = state.settings;
  if (!st) return mount(main, h('p', { class: 'empty' }, 'Settings are not available.'));
  const inputs = new Map();
  const errors = new Map();
  const field = (f) => {
    const raw = st.config[f.key];
    const input = h('input', { type: 'number', id: `f-${f.key}`, min: f.min, max: f.max, step: f.step, value: f.percent ? +(raw * 100).toFixed(2) : raw });
    const err = h('p', { class: 'err', hidden: true });
    inputs.set(f.key, input);
    errors.set(f.key, err);
    return h('div', { class: 'field', dataset: { key: f.key } },
      h('label', { for: `f-${f.key}` }, f.label), input,
      h('span', { class: 'hint' }, `${f.hint} (${f.min} to ${f.max})`), err);
  };
  const fill = (preset) => {
    for (const f of CUSTOM_FIELDS) {
      const v = preset.config[f.key];
      inputs.get(f.key).value = f.percent ? +(v * 100).toFixed(2) : v;
    }
  };
  const saveCustom = async (button) => {
    const values = {};
    let ok = true;
    for (const f of CUSTOM_FIELDS) {
      const n = Number(inputs.get(f.key).value);
      const bad = inputs.get(f.key).value.trim() === '' || !Number.isFinite(n) || n < f.min || n > f.max;
      inputs.get(f.key).closest('.field').classList.toggle('invalid', bad);
      errors.get(f.key).hidden = !bad;
      errors.get(f.key).textContent = bad ? `Enter a number from ${f.min} to ${f.max}.` : '';
      if (bad) ok = false;
      values[f.key] = f.percent ? n / 100 : n;
    }
    if (!ok) return;
    values.phash_max = Math.round(values.phash_max);
    values.dhash_max = Math.round(values.dhash_max);
    const hadResults = state.groups.length > 0 || state.lastSummary != null;
    state.settings = await call('set_settings', { preset: 'custom', custom: values });
    state.pendingPreset = null;
    forgetResults();
    if (hadResults) return startRegroup();
    toast('Saved. It applies to your next scan.');
    renderSettings();
  };
  const sizeInput = h('input', { type: 'number', id: 'f-min', min: 0, step: 1, value: +(st.min_size / 1024).toFixed(2) });
  const saveSize = async (button) => {
    const kb = Number(sizeInput.value);
    if (sizeInput.value.trim() === '' || !Number.isFinite(kb) || kb < 0) return toast('Enter a size in KB, 0 or more.', 'warn');
    state.settings = await call('set_settings', { min_size: Math.max(1, Math.round(kb * 1024)) });
    forgetResults();
    renderNav();
    toast('Saved. Scan again to apply it.');
    renderSettings();
  };
  mount(main, h('div', { class: 'settings stack' },
    h('h1', {}, 'Settings'),
    h('section', { class: 'panel' },
      h('h2', {}, 'Advanced matching'),
      h('p', { class: 'muted' }, 'Four values decide whether two photos count as the same picture. The presets on the Overview set them for you; set your own here. Exact copies (identical bytes) are always found.'),
      h('div', { class: 'row' }, h('span', { class: 'muted small' }, 'Start from:'),
        st.presets.filter((p) => p.key !== 'custom' && p.key !== 'exact_only').map((p) => h('button', { class: 'chip', onclick: () => fill(p) }, p.name))),
      CUSTOM_FIELDS.map(field),
      h('p', { class: 'row' }, h('button', { class: 'primary', onclick: (e) => busy(e.currentTarget, () => saveCustom(e.currentTarget)) }, 'Save and re-run'))),
    h('section', { class: 'panel' },
      h('h2', {}, 'Scanning'),
      h('div', { class: 'field' },
        h('label', { for: 'f-min' }, 'Smallest file to include (KB)'), sizeInput,
        h('span', { class: 'hint' }, 'Files smaller than this are skipped. Takes effect on the next scan.')),
      h('p', { class: 'row' }, h('button', { onclick: (e) => busy(e.currentTarget, () => saveSize(e.currentTarget)) }, 'Save'))),
    h('section', { class: 'panel' },
      h('h2', {}, 'About'),
      h('p', { class: 'muted' }, `DupeLens ${state.info.version ?? ''}. Everything runs on this computer: no account, no upload, no tracking. Your folder list, scan results and history are kept in one local database.`))));
}

// ---------- start-up ----------

async function start() {
  renderNav();
  try {
    state.info = await call('info');
    document.getElementById('version').textContent = `v${state.info.version}`;
    const status = await call('scan_status');
    if (status.state === 'running') return showScan();
    state.lastSummary = status.state === 'done' ? status.summary : null;
  } catch (e) {
    if (e instanceof ApiError && e.kind === 'offline') {
      mount(main, h('section', { class: 'panel narrow center' },
        h('h1', {}, 'Open DupeLens as the app'),
        h('p', { class: 'lead' }, e.message)));
      return;
    }
    showError(e);
  }
  // Offer results from an earlier session without making the user rescan.
  try {
    await loadGroups();
  } catch {
    // Not available yet (e.g. never scanned); the overview already works.
  }
  await showOverview();
}

// The stacked bar's segment widths are set from script (inline styles are not allowed by the
// page's Content-Security-Policy), so size them whenever the overview is drawn.
new MutationObserver(() => {
  if (state.screen === 'overview') sizeTypeBar();
}).observe(main, { childList: true });

start();
