// gamekit wired into panote.
//
// The engine is framework-agnostic and event-sourced, so persistence here is simply the
// event log kept in localStorage (the webview has it — notes.ts already uses it) and replayed
// on startup. That gives real persistence across app restarts with no Rust. For multi-device
// or hardened storage, swap the memory stores for a Rust/sqlx adapter behind Tauri `invoke`
// (the four StreakStore/EventStore/… ports) — the definitions and this API stay identical.

import {
  type Engine,
  type Event,
  type Rule,
  type Snapshot,
  createEngine,
  getPath,
  matchFilter,
  memoryAchievementStore,
  memoryEventStore,
  memoryScoreStore,
  memoryStreakStore,
  systemClock,
} from "@gamekit/core";
import { writable } from "svelte/store";
import { ACTOR, definitions } from "./definitions";

const EVENTS_KEY = "panote-gamekit-events";
const LASTACTIVE_KEY = "panote-gamekit-lastactive";
// Compaction trigger. The log is the only record of the user's history, so going
// over the cap folds it into a snapshot rather than truncating it.
const MAX_EVENTS = 300;
// day.active/day.missed ids are date-keyed and re-emitted on every save that day,
// so the log is what stops a restart from ticking the streak a second time. Only
// events from the last few days can still collide.
const KEEP_STREAK_DAYS = 3;

export interface Badge {
  code: string;
  name: string;
  description: string;
  emoji: string;
  rarity: number;
  earned: boolean;
}

export interface GameSnapshot {
  notes: number;
  words: number;
  tier: { current: string | null; next: string | null; remaining: number };
  streak: { current: number; best: number };
  badges: Badge[];
  earnedCount: number;
}

const empty: GameSnapshot = {
  notes: 0,
  words: 0,
  tier: { current: null, next: null, remaining: 0 },
  streak: { current: 0, best: 0 },
  badges: [],
  earnedCount: 0,
};

export const gameStats = writable<GameSnapshot>(empty);

let engine: Engine | null = null;
let log: Event[] = [];
/** Mirrors `log`'s ids so dedupe is a lookup, not a scan of the whole history. */
let seen = new Set<string>();
/** State of every event already folded out of `log`. Replaces them on load. */
let baseline: Snapshot | null = null;

const browser = () => typeof window !== "undefined" && typeof localStorage !== "undefined";

/** Older builds stored a bare `Event[]`; read that shape too. */
function loadPersisted(): { snapshot: Snapshot | null; events: Event[] } {
  try {
    const raw = JSON.parse(localStorage.getItem(EVENTS_KEY) ?? "[]");
    if (Array.isArray(raw)) return { snapshot: null, events: raw as Event[] };
    return { snapshot: raw.snapshot ?? null, events: (raw.events ?? []) as Event[] };
  } catch {
    return { snapshot: null, events: [] };
  }
}

function persist() {
  localStorage.setItem(EVENTS_KEY, JSON.stringify({ snapshot: baseline, events: log }));
}

const newEngine = () =>
  createEngine({
    events: memoryEventStore(),
    scores: memoryScoreStore(),
    achievements: memoryAchievementStore(),
    streaks: memoryStreakStore(),
    clock: systemClock,
    definitions,
  });

function dayString(ts: number) {
  return new Date(ts).toISOString().slice(0, 10); // YYYY-MM-DD (UTC)
}

/** Lazily build the engine and replay the persisted log. Safe to call repeatedly / during SSR. */
export async function initGamekit(): Promise<void> {
  if (engine || !browser()) return;
  engine = newEngine();
  const saved = loadPersisted();
  baseline = saved.snapshot;
  log = saved.events;
  seen = new Set(log.map((e) => e.id));
  if (baseline) await engine.seed(baseline);
  if (log.length) await engine.replay(log);
  await refresh();
}

async function append(event: Event): Promise<string[]> {
  if (!engine) return [];
  // The log mirrors what the engine has replayed/seen; a duplicate id is a no-op either way.
  if (seen.has(event.id)) return [];
  const r = await engine.emit(event);
  seen.add(event.id);
  log.push(event);
  return r.unlocked;
}

/**
 * Which raw events an unearned rule can still be waiting on. Score and streak
 * rules read projections, which the snapshot carries, so they need none — and a
 * threshold rule never needs more events than its own threshold.
 */
function neededIds(rule: Rule, out: Set<string>): void {
  switch (rule.kind) {
    case "all":
    case "any":
      for (const r of rule.rules) neededIds(r, out);
      return;
    case "score":
    case "streak":
      return;
    case "count": {
      // Unearned means fewer than `gte` matches exist, so this keeps all of them.
      const hits = log.filter(
        (e) => e.type === rule.eventType && (!rule.where || matchFilter(e, rule.where)),
      );
      for (const e of hits.slice(-rule.gte)) out.add(e.id);
      return;
    }
    case "unique": {
      // Only distinctness matters, so one event per value is the whole signal.
      const perValue = new Map<unknown, string>();
      for (const e of log) if (e.type === rule.eventType) perValue.set(getPath(e, rule.by), e.id);
      for (const id of perValue.values()) out.add(id);
      return;
    }
    default: {
      // Unfamiliar rule shape: keep everything it could read rather than guess.
      const type = (rule as { eventType?: string }).eventType;
      for (const e of log) if (e.type === type) out.add(e.id);
    }
  }
}

const streakEventTypes = new Set(
  (definitions.streaks ?? []).flatMap((s) => [...s.tickEvents, ...s.resetEvents]),
);

async function snapshotOf(e: Engine): Promise<Snapshot> {
  return {
    scores: await Promise.all(
      (definitions.scores ?? []).map(async (score) => ({
        actor: ACTOR,
        score,
        value: await e.score(ACTOR, score),
      })),
    ),
    achievements: (await e.achievements(ACTOR)).map((a) => ({ actor: ACTOR, ...a })),
    streaks: await Promise.all(
      (definitions.streaks ?? []).map(async (s) => ({
        actor: ACTOR,
        code: s.code,
        ...(await e.streak(ACTOR, s.code)),
      })),
    ),
  };
}

/**
 * Fold the events nothing depends on any more into `baseline`, so the log stops
 * growing with every save without costing a single point, badge or streak day.
 * The dropped events are replayed into a throwaway engine and that engine's state
 * becomes the new baseline, so `seed(baseline) + replay(log)` still reconstructs
 * exactly what a full replay would have.
 */
async function compact(): Promise<void> {
  if (!engine || log.length <= MAX_EVENTS) return;

  const earned = new Set((await engine.achievements(ACTOR)).map((a) => a.code));
  const keepIds = new Set<string>();
  for (const a of definitions.achievements ?? []) {
    if (!earned.has(a.code)) neededIds(a.rule, keepIds);
  }
  const since = Date.now() - KEEP_STREAK_DAYS * 86_400_000;
  const keep = (e: Event) =>
    keepIds.has(e.id) || (streakEventTypes.has(e.type) && e.ts >= since);

  const dropped = log.filter((e) => !keep(e));
  if (!dropped.length) return;

  const folded = newEngine();
  if (baseline) await folded.seed(baseline);
  await folded.replay(dropped);
  baseline = await snapshotOf(folded);
  // A badge the user already holds must never depend on the fold re-deriving it.
  baseline.achievements = (await engine.achievements(ACTOR)).map((a) => ({ actor: ACTOR, ...a }));

  log = log.filter(keep);
  seen = new Set(log.map((e) => e.id));
}

/**
 * Markdown source is not prose. Left raw, one `<mark data-color=...>` or a table
 * row counted as several "words written".
 */
function plainText(md: string): string {
  return md
    .replace(/```[\s\S]*?```/g, " ")                          // fenced code
    .replace(/<[^>]*>/g, " ")                                 // html tags and their attributes
    .replace(/!\[[^\]]*\]\([^)]*\)/g, " ")                    // images
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")                  // links keep their label
    .replace(/`([^`]*)`/g, "$1")                              // inline code keeps its text
    .replace(/^[ \t]*[|:-][|:\s-]*$/gm, " ")                  // rules and table separator rows
    .replace(/^[ \t]*(#{1,6}|>|[-*+]|\d+[.)])[ \t]+/gm, "")   // heading/quote/list markers
    .replace(/[*_~]/g, "")                                    // emphasis: dropped, not spaced,
                                                              // so snake_case stays one word
    .replace(/\|/g, " ");                                     // table cell separators
}

export function countWords(kind: string, content: unknown): number {
  if (kind === "document") {
    const trimmed = plainText((content as { body?: string })?.body ?? "").trim();
    return trimmed ? trimmed.split(/\s+/).length : 0;
  }
  return 0;
}

/**
 * Record a note save. Returns any newly unlocked badge codes (for a toast).
 * Emits note.created (new notes only), note.saved, and the daily-streak tick — emitting
 * day.missed first if a calendar day was skipped since the last active day.
 */
export async function recordNoteSaved(args: {
  isNew: boolean;
  kind: string;
  content: unknown;
}): Promise<string[]> {
  if (!browser()) return [];
  await initGamekit();
  if (!engine) return [];

  const now = Date.now();
  const today = dayString(now);
  const words = countWords(args.kind, args.content);
  const unlocked: string[] = [];

  // Daily streak bookkeeping: a gap of >1 day breaks the streak.
  const last = localStorage.getItem(LASTACTIVE_KEY);
  if (last && last !== today) {
    const gapDays = Math.round((Date.parse(today) - Date.parse(last)) / 86_400_000);
    if (gapDays > 1) {
      unlocked.push(...(await append({ id: `miss:${today}`, actor: ACTOR, type: "day.missed", ts: now, payload: {} })));
    }
  }

  const id = crypto.randomUUID();
  if (args.isNew) {
    unlocked.push(
      ...(await append({ id: `created:${id}`, actor: ACTOR, type: "note.created", ts: now, payload: { kind: args.kind } })),
    );
  }
  unlocked.push(
    ...(await append({ id: `saved:${id}`, actor: ACTOR, type: "note.saved", ts: now, payload: { kind: args.kind, words } })),
  );
  // day.active is idempotent per calendar day — only the first save of the day ticks the streak.
  unlocked.push(
    ...(await append({ id: `day:${today}`, actor: ACTOR, type: "day.active", ts: now, payload: {} })),
  );
  localStorage.setItem(LASTACTIVE_KEY, today);

  // One write per save instead of one per event, after compaction has had its say.
  await compact();
  persist();
  await refresh();
  return unlocked;
}

async function refresh(): Promise<void> {
  if (!engine) return;
  const notes = await engine.score(ACTOR, "notes");
  const words = await engine.score(ACTOR, "words");
  const tier = await engine.tier(ACTOR, "writer");
  const streak = await engine.streak(ACTOR, "daily");
  const owned = new Set((await engine.achievements(ACTOR)).map((a) => a.code));
  const badges: Badge[] = (definitions.achievements ?? []).map((a) => ({
    code: a.code,
    name: a.name,
    description: a.description,
    emoji: (a.metadata?.emoji as string) ?? "🏅",
    rarity: a.rarity,
    earned: owned.has(a.code),
  }));
  gameStats.set({
    notes,
    words,
    tier: { current: tier.current, next: tier.next, remaining: tier.remaining },
    streak: { current: streak.current, best: streak.best },
    badges,
    earnedCount: badges.filter((b) => b.earned).length,
  });
}
