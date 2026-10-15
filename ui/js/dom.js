// A tiny DOM builder. Text always goes in via textContent, never innerHTML, so a file
// named `<img onerror=...>.jpg` is shown as text and can't run code.

/**
 * h('button', { class: 'primary', onclick: fn, disabled: true }, 'Scan')
 * Props starting with "on" become event listeners; `class` and `dataset` are special;
 * false/null/undefined props and children are skipped.
 */
export function h(tag, props = {}, ...children) {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(props ?? {})) {
    if (value === false || value == null) continue;
    if (key.startsWith('on')) el.addEventListener(key.slice(2), value);
    else if (key === 'class') el.className = value;
    else if (key === 'dataset') Object.assign(el.dataset, value);
    else if (value === true) el.setAttribute(key, '');
    else el.setAttribute(key, String(value));
  }
  append(el, children);
  return el;
}

function append(el, children) {
  for (const child of children) {
    if (child == null || child === false) continue;
    if (Array.isArray(child)) append(el, child);
    else el.append(child instanceof Node ? child : document.createTextNode(String(child)));
  }
}

/** Replaces everything inside `el`. */
export function mount(el, ...children) {
  el.replaceChildren();
  append(el, children);
}
