// @vitest-environment happy-dom
//
// Two separate promises this module makes and used to break:
//   1. "words written" is a count of the user's PROSE. It was counting raw
//      markdown, so a highlight or a table inflated the number.
//   2. The stats survive a restart. That has to keep holding once the event log
//      is compacted, or compaction is just data loss with extra steps.
import { describe, it, expect, beforeEach, vi } from "vitest";
import { get } from "svelte/store";

const EVENTS_KEY = "panote-gamekit-events";
const LASTACTIVE_KEY = "panote-gamekit-lastactive";

// A fresh module instance == an app restart: `engine` and `log` are module-level.
async function boot() {
  vi.resetModules();
  const m = await import("./store");
  await m.initGamekit();
  return m;
}

function persistedEvents(): unknown[] {
  const raw = JSON.parse(localStorage.getItem(EVENTS_KEY) ?? "[]");
  return Array.isArray(raw) ? raw : (raw.events ?? []);
}

beforeEach(() => {
  localStorage.removeItem(EVENTS_KEY);
  localStorage.removeItem(LASTACTIVE_KEY);
});

describe("countWords", () => {
  let countWords: (kind: string, content: unknown) => number;

  beforeEach(async () => {
    ({ countWords } = await import("./store"));
  });

  const doc = (body: string) => countWords("document", { body });

  it("counts prose", () => {
    expect(doc("hello world")).toBe(2);
  });

  it("does not count HTML attributes as words", () => {
    // What the highlight button actually writes into the body.
    expect(
      doc('A <mark data-color="#ffe58f" style="background-color: #ffe58f">highlighted phrase</mark> here'),
    ).toBe(4);
  });

  it("does not count fenced code as prose", () => {
    expect(doc("Intro text\n\n```js\nconst a = 1;\nconsole.log(a);\n```\n\nOutro")).toBe(3);
  });

  it("counts table cells, not the pipes and rules around them", () => {
    expect(doc("| a | b |\n|---|---|\n| c | d |")).toBe(4);
  });

  it("does not count heading and emphasis markers", () => {
    expect(doc("## Big **bold** title")).toBe(3);
  });

  it("does not count list bullets", () => {
    expect(doc("- one\n- two")).toBe(2);
  });

  it("keeps a hyphenated or snake_case word whole", () => {
    expect(doc("well-known snake_case")).toBe(2);
  });

  it("scores nothing for kinds that are not prose", () => {
    expect(countWords("kanban", { body: "a b c" })).toBe(0);
    expect(doc("   ")).toBe(0);
  });
});

describe("event log", () => {
  // Enough saves to blow past the 300-event cap and past `centurion` at 100, so
  // compaction has to cope with rules going from pending to earned. Kept as low
  // as that allows: every emit rescans the engine's whole event store, so the
  // cost here is quadratic in SAVES.
  const SAVES = 160;
  // Booting the engine repeatedly is slow when the whole suite is competing for
  // the CPU; these assert behaviour, never latency.
  const SLOW = 30_000;

  async function fill(m: Awaited<ReturnType<typeof boot>>) {
    for (let i = 0; i < SAVES; i++) {
      await m.recordNoteSaved({ isNew: true, kind: "document", content: { body: "one two three" } });
    }
  }

  it("stays bounded instead of growing with every save", async () => {
    const m = await boot();
    await fill(m);
    expect(persistedEvents().length).toBeLessThanOrEqual(300);
  }, SLOW);

  it("keeps lifetime stats intact across a restart", async () => {
    const m = await boot();
    await fill(m);
    const before = get(m.gameStats);
    expect(before.words).toBe(SAVES * 3);
    expect(before.notes).toBe(SAVES);
    expect(before.earnedCount).toBeGreaterThan(0);

    const restarted = await boot();
    const after = get(restarted.gameStats);
    expect(after.words).toBe(before.words);
    expect(after.notes).toBe(before.notes);
    expect(after.badges.filter(b => b.earned).map(b => b.code).sort())
      .toEqual(before.badges.filter(b => b.earned).map(b => b.code).sort());
  }, SLOW);

  it("does not double-tick today's streak when the app restarts", async () => {
    const m = await boot();
    await m.recordNoteSaved({ isNew: true, kind: "document", content: { body: "a" } });
    expect(get(m.gameStats).streak.current).toBe(1);

    const restarted = await boot();
    await restarted.recordNoteSaved({ isNew: false, kind: "document", content: { body: "b" } });
    expect(get(restarted.gameStats).streak.current).toBe(1);
  }, SLOW);

  it("still reads a log written by the previous format", async () => {
    const m = await boot();
    await m.recordNoteSaved({ isNew: true, kind: "document", content: { body: "one two" } });
    // Downgrade the stored value to the bare array the old build wrote.
    localStorage.setItem(EVENTS_KEY, JSON.stringify(persistedEvents()));

    const restarted = await boot();
    expect(get(restarted.gameStats).words).toBe(2);
    expect(get(restarted.gameStats).notes).toBe(1);
  }, SLOW);
});
