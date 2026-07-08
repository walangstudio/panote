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
  createEngine,
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

const browser = () => typeof window !== "undefined" && typeof localStorage !== "undefined";

function loadLog(): Event[] {
  try {
    return JSON.parse(localStorage.getItem(EVENTS_KEY) ?? "[]") as Event[];
  } catch {
    return [];
  }
}

function persist() {
  localStorage.setItem(EVENTS_KEY, JSON.stringify(log));
}

function dayString(ts: number) {
  return new Date(ts).toISOString().slice(0, 10); // YYYY-MM-DD (UTC)
}

/** Lazily build the engine and replay the persisted log. Safe to call repeatedly / during SSR. */
export async function initGamekit(): Promise<void> {
  if (engine || !browser()) return;
  engine = createEngine({
    events: memoryEventStore(),
    scores: memoryScoreStore(),
    achievements: memoryAchievementStore(),
    streaks: memoryStreakStore(),
    clock: systemClock,
    definitions,
  });
  log = loadLog();
  if (log.length) await engine.replay(log);
  await refresh();
}

async function append(event: Event): Promise<string[]> {
  if (!engine) return [];
  // The log mirrors what the engine has replayed/seen; a duplicate id is a no-op either way.
  if (log.some((e) => e.id === event.id)) return [];
  const r = await engine.emit(event);
  log.push(event);
  persist();
  return r.unlocked;
}

export function countWords(kind: string, content: unknown): number {
  if (kind === "document") {
    const body = (content as { body?: string })?.body ?? "";
    const trimmed = body.trim();
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
