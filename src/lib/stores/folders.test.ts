// @vitest-environment happy-dom
//
// The tree is rendered recursively, so a malformed row is not a cosmetic
// problem - an orphan or a cycle would hang the sidebar. The backend rejects
// both, but this is the layer that has to survive a row that got in anyway.
import { describe, it, expect } from "vitest";
import type { Folder } from "$lib/tauri";
import { buildTree } from "./folders";

const f = (id: string, parent: string | null = null, note_count = 0): Folder =>
  ({ id, parent_id: parent, name: id, note_count });

const ids = (nodes: { id: string }[]) => nodes.map(n => n.id);

describe("buildTree", () => {
  it("returns nothing for no folders", () => {
    expect(buildTree([])).toEqual([]);
  });

  it("puts parentless folders at the root", () => {
    expect(ids(buildTree([f("a"), f("b")]))).toEqual(["a", "b"]);
  });

  it("nests a child under its parent", () => {
    const tree = buildTree([f("a"), f("b", "a")]);
    expect(ids(tree)).toEqual(["a"]);
    expect(ids(tree[0].children)).toEqual(["b"]);
  });

  it("builds several levels", () => {
    const tree = buildTree([f("a"), f("b", "a"), f("c", "b")]);
    expect(ids(tree[0].children[0].children)).toEqual(["c"]);
  });

  it("sorts siblings by name at every level", () => {
    const tree = buildTree([f("b"), f("a"), f("z", "a"), f("y", "a")]);
    expect(ids(tree)).toEqual(["a", "b"]);
    expect(ids(tree[0].children)).toEqual(["y", "z"]);
  });

  it("does not care what order the rows arrive in", () => {
    const tree = buildTree([f("c", "b"), f("b", "a"), f("a")]);
    expect(ids(tree)).toEqual(["a"]);
    expect(ids(tree[0].children[0].children)).toEqual(["c"]);
  });

  // A folder whose parent was deleted out from under it would otherwise vanish
  // from the tree while still existing - better to drop it than to render a
  // broken branch, and the backend cascade means it should not happen.
  it("drops a folder whose parent is missing", () => {
    const tree = buildTree([f("a"), f("orphan", "gone")]);
    expect(ids(tree)).toEqual(["a"]);
  });

  // The important one: a cycle must not hang the render.
  it("survives a two-node cycle without looping", () => {
    const tree = buildTree([f("a", "b"), f("b", "a")]);
    // Neither is reachable from a root, so the tree is empty rather than infinite.
    expect(tree).toEqual([]);
  });

  it("survives a self-parented folder", () => {
    expect(buildTree([f("a", "a")])).toEqual([]);
  });

  it("keeps the healthy part of a tree that also contains a cycle", () => {
    const tree = buildTree([f("ok"), f("x", "y"), f("y", "x")]);
    expect(ids(tree)).toEqual(["ok"]);
  });

  it("stops nesting at the depth cap", () => {
    const rows = [f("d0")];
    for (let i = 1; i < 30; i++) rows.push(f(`d${i}`, `d${i - 1}`));

    let node = buildTree(rows, 5)[0];
    let depth = 1;
    while (node.children.length) { node = node.children[0]; depth++; }
    expect(depth).toBe(5);
  });
});

describe("note counts", () => {
  it("reports a leaf's own count", () => {
    expect(buildTree([f("a", null, 3)])[0].totalCount).toBe(3);
  });

  // A collapsed folder should say how much is inside it, not just at its top.
  it("rolls subfolder counts up into the parent", () => {
    const tree = buildTree([f("a", null, 1), f("b", "a", 2), f("c", "b", 4)]);
    expect(tree[0].totalCount).toBe(7);
    expect(tree[0].children[0].totalCount).toBe(6);
  });

  it("keeps sibling subtrees separate", () => {
    const tree = buildTree([f("a", null, 0), f("b", "a", 2), f("c", null, 5)]);
    expect(tree[0].totalCount).toBe(2);
    expect(tree[1].totalCount).toBe(5);
  });

  it("leaves note_count as the folder's own tally", () => {
    const tree = buildTree([f("a", null, 1), f("b", "a", 2)]);
    expect(tree[0].note_count).toBe(1);
    expect(tree[0].totalCount).toBe(3);
  });
});
