// Which photos the user has marked for deletion, per duplicate group. A port of the Flutter
// app's SelectionModel (app/lib/src/selection.dart), so both UIs behave the same.
//
// Starts in the PRD's default state: in every group, everything except the recommended
// copy is marked. Nothing is deleted until the user confirms on the review screen, and the
// engine re-checks every rule (decision 0006) no matter what the UI sends.

/** "Keep Recommended": every member except the recommended one. */
export function defaultDeletion(group) {
  return new Set(group.members.map((m) => m.id).filter((id) => id !== group.recommended_id));
}

export class Selection {
  constructor(groups = []) {
    this.groups = [];
    this.marked = new Map();
    this.load(groups);
  }

  /**
   * Loads new results. With `keepPrevious`, a group seen before keeps the user's choices
   * (minus files that are gone), so a partial delete doesn't reset their work.
   */
  load(groups, { keepPrevious = false } = {}) {
    const previous = this.marked;
    this.groups = groups;
    this.marked = new Map();
    for (const g of groups) {
      const ids = new Set(g.members.map((m) => m.id));
      const old = keepPrevious ? previous.get(g.id) : undefined;
      const kept = old ? new Set([...old].filter((id) => ids.has(id))) : defaultDeletion(g);
      // Never start with every copy marked.
      this.marked.set(g.id, kept.size >= ids.size ? defaultDeletion(g) : kept);
    }
  }

  isMarked(group, fileId) {
    return this.marked.get(group.id)?.has(fileId) ?? false;
  }

  markedIn(group) {
    return this.marked.get(group.id) ?? new Set();
  }

  /**
   * Flips one photo between keep and delete. Returns false (and changes nothing) if it
   * would mark every copy in the group, since at least one copy must be kept.
   */
  toggle(group, fileId) {
    const set = this.marked.get(group.id) ?? new Set();
    this.marked.set(group.id, set);
    if (set.has(fileId)) {
      set.delete(fileId);
      return true;
    }
    if (set.size + 1 >= group.members.length) return false;
    set.add(fileId);
    return true;
  }

  /** Per-group "Restore": back to the recommended default. */
  restore(group) {
    this.marked.set(group.id, defaultDeletion(group));
  }

  /** Per-group "Keep all": delete nothing in this group. */
  keepAll(group) {
    this.marked.set(group.id, new Set());
  }

  /** Batch "Keep recommended" for the given groups (default: all). */
  keepRecommended(groups = this.groups) {
    for (const g of groups) this.restore(g);
  }

  /** Batch "Clear selection" for the given groups (default: all). */
  clear(groups = this.groups) {
    for (const g of groups) this.keepAll(g);
  }

  /** Every marked file, in group order, as { group, file }. */
  markedFiles() {
    const out = [];
    for (const g of this.groups) {
      for (const m of g.members) if (this.isMarked(g, m.id)) out.push({ group: g, file: m });
    }
    return out;
  }

  markedBytes() {
    return this.markedFiles().reduce((sum, { file }) => sum + file.size, 0);
  }
}
