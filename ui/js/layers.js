// Makes the Android Back button close the top-most thing (a preview, a sheet, a dialog) before it
// leaves a screen. Each open layer adds one browser-history entry; Back removes it, and the
// page's `popstate` handler (app.js) then runs the layer's teardown. Closing a layer from the
// page itself goes the same way (`closeTopLayer`), so there is only one path.

const stack = [];

/** Registers an open layer. `teardown` removes it from the page; it runs when the layer closes. */
export function pushLayer(teardown) {
  stack.push(teardown);
  history.pushState({ layer: true }, '');
}

/** Closes the top-most layer, as if Back had been pressed. */
export function closeTopLayer() {
  history.back();
}

/** Called on `popstate`. True if a layer was closed (so the screen should stay as it is). */
export function popLayer() {
  const teardown = stack.pop();
  if (!teardown) return false;
  teardown();
  return true;
}
