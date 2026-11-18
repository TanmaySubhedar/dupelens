// The phone's screens: results as a gallery-style grid, selection, a full-screen preview, the
// delete confirmation and the folder browser. Used only on Android (app.js decides); the desktop
// keeps its own pages. Design notes: docs/decisions/0016-android-app.md.
//
// Everything shared with app.js arrives through `createPhone(ctx)`, so this file never imports
// app.js (that would be a circular import).

import { browseFolder } from './api.js';
import { closeTopLayer, pushLayer } from './layers.js';

const SVG = 'http://www.w3.org/2000/svg';
// Material icon outlines (Apache 2.0), 24 x 24.
const ICONS = {
  close: 'M19 6.41 17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z',
  check: 'M9 16.17 4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z',
  back: 'M15.41 7.41 14 6l-6 6 6 6 1.41-1.41L10.83 12z',
  forward: 'M10 6 8.59 7.41 13.17 12l-4.58 4.59L10 18l6-6z',
  // The two diagonal arrows ("open in full").
  expand: 'M21 11V3h-8l3.29 3.29-10 10L3 13v8h8l-3.29-3.29 10-10z',
  filter: 'M10 18h4v-2h-4v2zM3 6v2h18V6H3zm3 7h12v-2H6v2z',
  all: 'M18 7l-1.41-1.41-6.34 6.34 1.41 1.41L18 7zm4.24-1.41L11.66 16.17 7.48 12l-1.41 1.41L11.66 19l12-12-1.42-1.41zM.41 13.41 6 19l1.41-1.41L1.83 12 .41 13.41z',
  trash: 'M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z',
};

export function icon(name) {
  const svg = document.createElementNS(SVG, 'svg');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('width', '24');
  svg.setAttribute('height', '24');
  svg.setAttribute('aria-hidden', 'true');
  svg.setAttribute('class', 'svg-icon');
  const path = document.createElementNS(SVG, 'path');
  path.setAttribute('d', ICONS[name]);
  path.setAttribute('fill', 'currentColor');
  svg.append(path);
  return svg;
}

const clamp = (n, lo, hi) => Math.min(hi, Math.max(lo, n));

export function createPhone(ctx) {
  const {
    state, main, h, mount, call, enter, toast, busy, showError, preview, modal, perPage,
    formatBytes, plural, categoryName, visibleGroups, factRows, doDelete, startScan, showOverview,
  } = ctx;

  /** A round icon-only button. */
  const ibtn = (name, label, onclick, active = false) => h('button', {
    class: `ibtn${active ? ' active' : ''}`, 'aria-label': label, title: label, onclick,
  }, icon(name));

  // ---------- results ----------

  async function showResults() {
    enter('results');
    if (state.groups.length === 0) return showEmpty();
    if (state.filter !== 'all' && !state.groups.some((g) => g.category === state.filter)) state.filter = 'all';
    mount(main, h('div', { class: 'p-results' },
      h('div', { class: 'p-bar', id: 'p-bar' }),
      h('div', { class: 'p-groups', id: 'p-groups' })));
    renderBar();
    renderGroups();
  }

  /** Nothing left to review (or nothing found yet): say so briefly, offer another scan. */
  async function showEmpty() {
    const stats = await call('stats').catch(() => null);
    if (state.screen !== 'results') return;
    mount(main, h('section', { class: 'panel narrow center' },
      h('h1', {}, 'No duplicates'),
      stats && stats.bytes_freed > 0 && h('p', { class: 'muted' }, `${formatBytes(stats.bytes_freed)} freed so far`),
      h('p', {}, h('button', {
        class: 'primary big',
        onclick: (e) => busy(e.currentTarget, () => (state.folders.length === 0 ? showOverview() : startScan())),
      }, 'Scan again'))));
  }

  /** The bar above the grid: a summary, or (once something is ticked) the selection actions. */
  function renderBar() {
    const bar = document.getElementById('p-bar');
    if (!bar) return;
    const marked = state.selection.markedFiles();
    if (marked.length > 0) {
      bar.className = 'p-bar selecting';
      mount(bar,
        ibtn('close', 'Clear selection', clearAll),
        h('div', { class: 'p-count' },
          h('strong', {}, `${marked.length.toLocaleString()} selected`),
          h('span', { class: 'muted' }, formatBytes(state.selection.markedBytes()))),
        h('div', { class: 'spacer' }),
        ibtn('all', 'Select duplicates', selectDuplicates),
        h('button', { class: 'danger', onclick: confirmDelete }, icon('trash'), 'Delete'));
      return;
    }
    const groups = visibleGroups();
    const bytes = groups.reduce((n, g) => n + g.reclaimable_bytes, 0);
    bar.className = 'p-bar';
    mount(bar,
      h('div', { class: 'p-count' },
        h('strong', {}, plural(groups.length, 'group')),
        h('span', { class: 'muted' }, formatBytes(bytes))),
      h('div', { class: 'spacer' }),
      h('button', { class: 'link', onclick: selectDuplicates }, 'Select duplicates'),
      ibtn('filter', 'Filter', openFilter, state.filter !== 'all' || state.kind !== 'all'));
  }

  function renderGroups() {
    const list = document.getElementById('p-groups');
    if (!list) return;
    const groups = visibleGroups();
    const page = groups.slice(0, state.shown);
    mount(list,
      page.map(groupRow),
      groups.length === 0 && h('p', { class: 'empty' }, 'Nothing here'),
      groups.length > page.length && h('button', {
        class: 'more', onclick: () => { state.shown += perPage; renderGroups(); },
      }, `More (${groups.length - page.length})`));
  }

  function groupRow(group) {
    const similar = group.kind !== 'exact' || group.tentative;
    return h('section', { class: 'p-group', dataset: { id: group.id } },
      h('header', { class: 'p-ghead' },
        h('span', {}, plural(group.members.length, 'file')),
        h('span', { class: 'muted' }, formatBytes(group.reclaimable_bytes)),
        similar && h('span', { class: 'p-sim' }, 'Similar')),
      // The next copy peeks in from the right edge, hinting that the row scrolls sideways.
      h('div', { class: 'p-strip' }, group.members.map((file) => tile(group, file))));
  }

  let swallowClick = false;
  function tile(group, file) {
    const selected = state.selection.isMarked(group, file.id);
    return h('div', {
      class: `p-tile${selected ? ' selected' : ''}`, dataset: { file: file.id },
      role: 'checkbox', 'aria-checked': String(selected), 'aria-label': file.name,
      onclick: () => {
        if (swallowClick) { swallowClick = false; return; }
        toggle(group, file);
      },
      // A long press selects, like in a gallery (it fires the context menu; the first tap after
      // it must not undo the selection).
      oncontextmenu: (e) => {
        e.preventDefault();
        swallowClick = true;
        setTimeout(() => { swallowClick = false; }, 600);
        if (!state.selection.isMarked(group, file.id)) toggle(group, file);
      },
    },
    preview(file),
    h('span', { class: 'p-check' }, icon('check')),
    group.recommended_id === file.id && h('span', { class: 'p-best' }, 'Best'),
    h('button', {
      class: 'p-expand', 'aria-label': 'Preview',
      onclick: (e) => { e.stopPropagation(); openPreview(group, file); },
    }, icon('expand')));
  }

  /** Ticks or unticks one copy. A group must keep at least one. */
  function toggle(group, file) {
    if (!state.selection.toggle(group, file.id)) {
      toast('Keep one copy', 'warn');
      return false;
    }
    paintGroup(group);
    renderBar();
    return true;
  }

  /** Updates the tick marks in place (rebuilding the row would reset its sideways scroll). */
  function paintGroup(group) {
    const section = document.querySelector(`.p-group[data-id="${group.id}"]`);
    if (!section) return;
    for (const el of section.querySelectorAll('.p-tile')) {
      const on = state.selection.isMarked(group, Number(el.dataset.file));
      el.classList.toggle('selected', on);
      el.setAttribute('aria-checked', String(on));
    }
  }

  function paintAll() {
    for (const group of state.groups) paintGroup(group);
    renderBar();
  }

  function selectDuplicates() {
    state.selection.keepRecommended(visibleGroups());
    paintAll();
    if (state.selection.markedFiles().length === 0) toast('Pick these by hand', 'warn');
  }

  function clearAll() {
    state.selection.clear(state.groups);
    paintAll();
  }

  // ---------- filter ----------

  function openFilter() {
    modal((close) => {
      const body = h('div', { class: 'p-filter' });
      const draw = () => {
        const counts = new Map();
        for (const g of state.groups) counts.set(g.category, (counts.get(g.category) ?? 0) + 1);
        const chip = (active, label, onclick, n) => h('button', {
          class: active ? 'chip active' : 'chip', 'aria-pressed': String(active), onclick,
        }, label, n != null && h('span', { class: 'num' }, n.toLocaleString()));
        const setType = (key) => () => { state.filter = key; apply(); };
        const setKind = (key) => () => { state.kind = key; apply(); };
        mount(body,
          h('div', { class: 'chips' },
            chip(state.filter === 'all', 'All', setType('all'), state.groups.length),
            [...counts].map(([key, n]) => chip(state.filter === key, categoryName(key), setType(key), n))),
          h('div', { class: 'chips' },
            [['all', 'All'], ['exact', 'Exact'], ['near', 'Similar'], ['maybe', 'Maybe']]
              .map(([key, label]) => chip(state.kind === key, label, setKind(key)))),
          h('div', { class: 'dialog-buttons' }, h('button', { class: 'primary', onclick: () => close() }, 'Done')));
      };
      // The list behind the sheet updates as you choose.
      const apply = () => { state.shown = perPage; draw(); renderBar(); renderGroups(); };
      draw();
      return [body];
    }, { sheet: true });
  }

  // ---------- delete ----------

  async function confirmDelete() {
    const marked = state.selection.markedFiles();
    if (marked.length === 0) return;
    const permanent = !state.info.trash_supported;
    const shown = marked.slice(0, 4);
    const answer = await modal((close) => [
      h('h3', { class: 'sheet-title' }, `Delete ${plural(marked.length, 'file')}?`),
      h('p', { class: 'muted' }, `${formatBytes(state.selection.markedBytes())}${permanent ? ' · Can’t be undone' : ''}`),
      h('div', { class: 'p-minis' },
        shown.map(({ file }) => preview(file, { cls: 'mini' })),
        marked.length > shown.length && h('span', { class: 'muted' }, `+${(marked.length - shown.length).toLocaleString()}`)),
      h('div', { class: 'dialog-buttons' },
        h('button', { onclick: () => close(null) }, 'Cancel'),
        h('button', { class: 'danger', onclick: () => close('delete') }, 'Delete')),
    ], { sheet: true });
    if (answer !== 'delete') return;
    toast('Deleting…');
    try {
      await doDelete(marked, permanent);
    } catch (e) {
      showError(e);
    }
  }

  /** After a delete: say what happened and stay on the results (or the empty state). */
  function afterDelete(report) {
    toast(`${plural(report.deleted.length, 'file')} deleted · ${formatBytes(report.bytes_freed)} freed`);
    if (report.failed.length > 0) toast(`${plural(report.failed.length, 'file')} skipped: ${report.failed[0].reason}`, 'warn');
    return showResults();
  }

  // ---------- full-screen preview ----------

  function openPreview(group, start) {
    let index = group.members.findIndex((m) => m.id === start.id);
    let scale = 1;
    let tx = 0;
    let ty = 0;
    let infoOpen = false;

    const stage = h('div', { class: 'pv-stage' });
    const counter = h('div', { class: 'pv-counter' });
    const pick = h('button', { class: 'ibtn pv-pick', 'aria-label': 'Select', onclick: () => { toggle(group, current()); chrome(); } }, icon('check'));
    const grab = h('button', { class: 'pv-grab', 'aria-label': 'Close details' }, h('span'));
    const infoBody = h('div', { class: 'pv-body' });
    const info = h('div', { class: 'pv-info' }, grab, infoBody);
    const handle = h('button', { class: 'pv-handle', 'aria-label': 'Details', onclick: () => setInfo(true) }, h('span'));
    const root = h('div', { class: 'pv', role: 'dialog', 'aria-modal': 'true' },
      h('div', { class: 'pv-top' }, ibtn('close', 'Close', () => closeTopLayer()), counter, pick),
      stage, handle, info);
    document.body.append(root);
    pushLayer(() => {
      root.remove();
      paintGroup(group);
      renderBar();
    });

    const current = () => group.members[index];

    /** Everything that depends on which copy is showing. */
    function chrome() {
      const file = current();
      counter.textContent = `${index + 1} / ${group.members.length}`;
      const on = state.selection.isMarked(group, file.id);
      pick.classList.toggle('on', on);
      pick.setAttribute('aria-pressed', String(on));
      const rows = [['Path', file.path], ...factRows(group, file).filter(([k]) => k !== 'Type')];
      mount(infoBody, h('h3', {}, file.name), h('dl', { class: 'facts' }, rows.map(([k, v]) => [h('dt', {}, k), h('dd', {}, v)])));
    }

    function draw() {
      scale = 1; tx = 0; ty = 0;
      mount(stage, preview(current(), { large: true, cls: 'pv-img' }));
      chrome();
    }

    function setInfo(open) {
      infoOpen = open;
      info.classList.toggle('open', open);
      handle.hidden = open;
    }

    const apply = () => stage.firstChild?.style?.setProperty('transform', `translate(${tx}px, ${ty}px) scale(${scale})`);
    const clampPan = () => {
      const maxX = ((scale - 1) * stage.clientWidth) / 2;
      const maxY = ((scale - 1) * stage.clientHeight) / 2;
      tx = clamp(tx, -maxX, maxX);
      ty = clamp(ty, -maxY, maxY);
    };
    const go = (delta) => {
      const next = index + delta;
      if (next < 0 || next >= group.members.length) return;
      index = next;
      draw();
    };

    // Gestures on the picture: pinch or double-tap to zoom, drag to pan while zoomed; at normal
    // size swipe sideways for the other copies, swipe up from the bottom for the details, and
    // swipe down to close.
    const pointers = new Map();
    let from = null;
    let pinch = null;
    let lastTap = 0;
    const spread = () => {
      const [a, b] = [...pointers.values()];
      return Math.hypot(a.x - b.x, a.y - b.y);
    };

    stage.addEventListener('pointerdown', (e) => {
      stage.setPointerCapture(e.pointerId);
      pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (pointers.size === 1) {
        const box = stage.getBoundingClientRect();
        from = { x: e.clientX, y: e.clientY, bottom: e.clientY > box.top + box.height * 0.6 };
      } else if (pointers.size === 2) {
        pinch = { spread: spread(), scale };
        from = null;
      }
    });
    stage.addEventListener('pointermove', (e) => {
      const last = pointers.get(e.pointerId);
      if (!last) return;
      const dx = e.clientX - last.x;
      const dy = e.clientY - last.y;
      last.x = e.clientX;
      last.y = e.clientY;
      if (pointers.size === 2 && pinch) {
        scale = clamp((pinch.scale * spread()) / pinch.spread, 1, 5);
        if (scale === 1) { tx = 0; ty = 0; }
        clampPan();
        apply();
      } else if (from && scale > 1) {
        tx += dx;
        ty += dy;
        clampPan();
        apply();
      }
    });
    const release = (e) => {
      if (!pointers.delete(e.pointerId)) return;
      if (pointers.size < 2) pinch = null;
      if (!from || pointers.size > 0) return;
      const dx = e.clientX - from.x;
      const dy = e.clientY - from.y;
      const [adx, ady] = [Math.abs(dx), Math.abs(dy)];
      const fromBottom = from.bottom;
      from = null;
      const tap = adx < 10 && ady < 10;
      if (infoOpen) {
        if (tap || dy > 50) setInfo(false);
        return;
      }
      if (scale > 1) {
        if (tap && Date.now() - lastTap < 300) { scale = 1; tx = 0; ty = 0; apply(); }
        lastTap = tap ? Date.now() : 0;
        return;
      }
      if (tap) {
        if (Date.now() - lastTap < 300) { scale = 2.5; apply(); lastTap = 0; } else lastTap = Date.now();
      } else if (adx > 60 && adx > ady * 1.2) {
        go(dx < 0 ? 1 : -1);
      } else if (dy < -50 && ady > adx * 1.2 && fromBottom) {
        setInfo(true);
      } else if (dy > 80 && ady > adx * 1.2) {
        closeTopLayer();
      }
    };
    stage.addEventListener('pointerup', release);
    // A cancelled touch (the system took the gesture over) has no trustworthy end point:
    // forget it rather than treating it as a swipe.
    stage.addEventListener('pointercancel', (e) => {
      pointers.delete(e.pointerId);
      pinch = null;
      from = null;
    });

    // The grab bar of the details panel: tap or pull down to close it.
    let pull = null;
    grab.addEventListener('pointerdown', (e) => { grab.setPointerCapture(e.pointerId); pull = e.clientY; });
    grab.addEventListener('pointerup', (e) => {
      if (pull != null && (Math.abs(e.clientY - pull) < 10 || e.clientY - pull > 30)) setInfo(false);
      pull = null;
    });

    draw();
  }

  // ---------- folder browser ----------

  /**
   * Browse the phone's folders and tick any number of them, across folders. A row's body ticks
   * it; the arrow opens it. Resolves to the ticked paths, or null if cancelled.
   */
  function browseFolders() {
    const ticked = new Set();
    const added = new Set(state.folders.map((f) => f.toLowerCase()));
    const body = h('div', { class: 'browser' });
    return modal((close) => {
      const addButton = h('button', { class: 'primary', disabled: true, onclick: () => close([...ticked]) }, 'Add');
      const syncAdd = () => {
        addButton.disabled = ticked.size === 0;
        addButton.textContent = ticked.size ? `Add ${ticked.size}` : 'Add';
      };
      const footer = h('div', { class: 'dialog-buttons' }, h('button', { onclick: () => close(null) }, 'Cancel'), addButton);

      const open = async (path) => {
        mount(body, h('p', { class: 'muted pad' }, '…'));
        let listing;
        try {
          listing = await browseFolder(path);
        } catch (e) {
          mount(body, h('p', { class: 'err-text pad' }, e.message), footer);
          return;
        }
        const row = (f) => {
          const done = added.has(f.path.toLowerCase());
          const li = h('li', { class: `b-row${done ? ' added' : ''}${ticked.has(f.path) ? ' on' : ''}` });
          const pick = h('button', {
            class: 'b-pick', disabled: done, 'aria-pressed': String(ticked.has(f.path)),
            onclick: () => {
              if (ticked.has(f.path)) ticked.delete(f.path); else ticked.add(f.path);
              li.classList.toggle('on', ticked.has(f.path));
              pick.setAttribute('aria-pressed', String(ticked.has(f.path)));
              syncAdd();
            },
          }, h('span', { class: 'b-check' }, icon('check')), h('span', { class: 'name' }, f.name));
          li.append(pick, ibtn('forward', `Open ${f.name}`, () => open(f.path)));
          return li;
        };
        const title = listing.parent ? listing.path.split('/').filter(Boolean).pop() : 'Storage';
        mount(body,
          h('div', { class: 'b-head' },
            listing.parent ? ibtn('back', 'Back', () => open(listing.parent)) : h('span', { class: 'b-gap' }),
            h('strong', { class: 'b-title' }, title)),
          h('ul', { class: 'b-list' },
            listing.folders.map(row),
            listing.folders.length === 0 && h('li', { class: 'empty pad' }, 'Empty')),
          footer);
        syncAdd();
      };
      open(null);
      return [body];
    }, { full: true });
  }

  return { showResults, afterDelete, browseFolders };
}
