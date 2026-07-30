import { writable } from "svelte/store";
import { folderList, type Folder } from "$lib/tauri";

/// Flat list as the backend returns it; the tree is built for rendering.
export const folders = writable<Folder[]>([]);

export async function refreshFolders() {
  // A failure must not blank the tree — a folder vanishing from the sidebar
  // reads as data loss even when the notes are fine.
  try { folders.set(await folderList()); } catch { /* keep the cached tree */ }
}

export interface FolderNode extends Folder {
  children: FolderNode[];
  /// Notes in this folder and everything under it, which is what a folder send
  /// takes and what the count beside a collapsed folder should show.
  totalCount: number;
}

/// Build the tree, dropping anything whose parent is missing or that sits in a
/// cycle. The backend rejects both, but this renders recursively - one bad row
/// from an older database would hang the UI rather than look wrong.
export function buildTree(flat: Folder[], maxDepth = 20): FolderNode[] {
  const byId = new Map<string, FolderNode>(
    flat.map(f => [f.id, { ...f, children: [], totalCount: f.note_count }]),
  );
  const roots: FolderNode[] = [];

  for (const node of byId.values()) {
    const parent = node.parent_id ? byId.get(node.parent_id) : undefined;
    if (!node.parent_id) roots.push(node);
    else if (parent) parent.children.push(node);
    // else: orphan, deliberately dropped
  }

  // Depth-cap and roll counts up, iteratively — a cycle among non-roots is
  // simply never reached from a root, so it cannot loop.
  const prune = (nodes: FolderNode[], depth: number): number => {
    let sum = 0;
    for (const n of nodes) {
      if (depth >= maxDepth) n.children = [];
      n.totalCount = n.note_count + prune(n.children, depth + 1);
      sum += n.totalCount;
    }
    nodes.sort((a, b) => a.name.localeCompare(b.name));
    return sum;
  };
  prune(roots, 1);

  return roots;
}
