import {
  createEngine,
  fixedClock,
  memoryAchievementStore,
  memoryEventStore,
  memoryScoreStore,
  memoryStreakStore,
} from "@gamekit/core";
import { describe, expect, it } from "vitest";
import { ACTOR, definitions } from "./definitions";

const HOUR = 3_600_000;
const DAY = 86_400_000;

function makeEngine() {
  return createEngine({
    events: memoryEventStore(),
    scores: memoryScoreStore(),
    achievements: memoryAchievementStore(),
    streaks: memoryStreakStore(),
    clock: fixedClock(0),
    definitions,
  });
}

const created = (id: string, kind: string, ts = 0) => ({
  id,
  actor: ACTOR,
  type: "note.created",
  ts,
  payload: { kind },
});

describe("panote gamekit definitions", () => {
  it("counts notes and unlocks first_note", async () => {
    const engine = makeEngine();
    const r = await engine.emit(created("c1", "document"));
    expect(await engine.score(ACTOR, "notes")).toBe(1);
    expect(r.unlocked).toContain("first_note");
  });

  it("awards words from note.saved and unlocks wordsmith at 10k", async () => {
    const engine = makeEngine();
    await engine.emit({ id: "s1", actor: ACTOR, type: "note.saved", ts: 0, payload: { kind: "document", words: 9999 } });
    let owned = (await engine.achievements(ACTOR)).map((a) => a.code);
    expect(owned).not.toContain("wordsmith");
    await engine.emit({ id: "s2", actor: ACTOR, type: "note.saved", ts: 0, payload: { kind: "document", words: 1 } });
    owned = (await engine.achievements(ACTOR)).map((a) => a.code);
    expect(owned).toContain("wordsmith");
    expect(await engine.score(ACTOR, "words")).toBe(10_000);
  });

  it("night_owl fires for a save in the 00:00–04:00 window (todBetween)", async () => {
    const engine = makeEngine();
    const r = await engine.emit({ id: "s1", actor: ACTOR, type: "note.saved", ts: 2 * HOUR, payload: { kind: "document", words: 5 } });
    expect(r.unlocked).toContain("night_owl");
  });

  it("polymath needs all four note kinds (unique)", async () => {
    const engine = makeEngine();
    for (const k of ["document", "checklist", "kanban"]) await engine.emit(created(`c-${k}`, k));
    expect((await engine.achievements(ACTOR)).map((a) => a.code)).not.toContain("polymath");
    const r = await engine.emit(created("c-table", "table"));
    expect(r.unlocked).toContain("polymath");
  });

  it("daily streak ticks per day and powers the writer streak", async () => {
    const engine = makeEngine();
    for (let d = 0; d < 7; d++) {
      await engine.emit({ id: `day-${d}`, actor: ACTOR, type: "day.active", ts: d * DAY, payload: {} });
    }
    expect((await engine.streak(ACTOR, "daily")).current).toBe(7);
    expect((await engine.achievements(ACTOR)).map((a) => a.code)).toContain("week_warrior");
  });

  it("writer tier climbs with note count", async () => {
    const engine = makeEngine();
    for (let i = 0; i < 10; i++) await engine.emit(created(`n${i}`, "document"));
    expect((await engine.tier(ACTOR, "writer")).current).toBe("scribe");
  });
});
