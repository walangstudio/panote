// The Material Symbols font is subsetted to a fixed icon list in
// scripts/fetch-fonts.ps1, so the app can ship offline in a few KB instead of
// several MB. An icon added to the UI but not to that list does not fall back to
// a placeholder - the browser renders the ligature name as literal text, so the
// button reads "CONTENT_COPY" or "_NEW_FOLDER".
//
// That shipped twice: once on the credential table's reveal/copy buttons, again
// on the folder tree. It is invisible to every other test, because nothing else
// looks at the font. So this compares the icons the source uses against the
// icons the subset contains.
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dirname, "..");

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (p.endsWith(".svelte")) out.push(p);
  }
  return out;
}

/// The names the subset was generated from.
function subsetIcons(): Set<string> {
  const script = readFileSync(join(ROOT, "scripts/fetch-fonts.ps1"), "utf8");
  const block = script.match(/\$Icons = @\(([\s\S]*?)\) -join/);
  if (!block) throw new Error("could not find $Icons in scripts/fetch-fonts.ps1");
  return new Set([...block[1].matchAll(/"([a-z_0-9]+)"/g)].map(m => m[1]));
}

/// Ligatures rendered inside a material-symbols span, literal or chosen at
/// runtime.
///
/// The dynamic form matters more than the literal one: the two icons that
/// actually shipped broken were both ternaries
/// (`{revealed ? "visibility_off" : "visibility"}`), so a check that only read
/// literals passed while the credential table showed CONTENT_COPY as text.
function usedIcons(): Map<string, string> {
  const found = new Map<string, string>();
  // The span's body, whatever shape it takes.
  const span = /material-symbols-outlined[^>]*>([\s\S]{0,200}?)</g;
  for (const file of walk(join(ROOT, "src"))) {
    const rel = file.slice(ROOT.length + 1);
    const src = readFileSync(file, "utf8");
    for (const m of src.matchAll(span)) {
      const body = m[1].trim();
      const names = body.startsWith("{")
        // Dynamic: the quoted strings, minus the operands being compared
        // against. `mode === "remove" ? "lock_open" : ...` renders lock_open;
        // "remove" is the condition, not an icon.
        ? [...body.replace(/[!=]==?\s*"[^"]*"/g, "").matchAll(/"([a-z_][a-z_0-9]*)"/g)]
            .map(q => q[1])
        // Literal.
        : /^[a-z_][a-z_0-9]*$/.test(body)
          ? [body]
          : [];
      for (const n of names) if (!found.has(n)) found.set(n, rel);
    }
  }
  return found;
}

describe("material symbols subset", () => {
  it("finds the icon list and some usages, so the check is not vacuous", () => {
    expect(subsetIcons().size).toBeGreaterThan(50);
    expect(usedIcons().size).toBeGreaterThan(20);
  });

  it("contains every icon the markup renders directly", () => {
    const have = subsetIcons();
    const missing = [...usedIcons()]
      .filter(([icon]) => !have.has(icon))
      .map(([icon, file]) => `${icon} (${file})`);

    expect(
      missing,
      "these icons will render as literal text; add them to $Icons in scripts/fetch-fonts.ps1 and re-run it",
    ).toEqual([]);
  });

  // The generated CSS is what the app actually loads, so a stale regeneration is
  // as bad as a missing name.
  it("has a generated font css that mentions the symbols file", () => {
    const css = readFileSync(join(ROOT, "static/fonts/fonts.css"), "utf8");
    expect(css).toContain("Material Symbols Outlined");
    expect(css).toMatch(/material-symbols-\d+\.woff2/);
  });
});
