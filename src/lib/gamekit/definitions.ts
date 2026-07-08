// panote's gamification, declared for the gamekit engine.
//
// Single-user desktop app, so there's one local actor and no leaderboards. Showcases the
// generic engine — including gamekit 0.2's `count.todBetween` (night_owl) — running inside
// a Tauri webview with zero platform-specific code.

import type { Definitions } from "@gamekit/core";

const HOUR = 3_600_000;

export const ACTOR = "me";

export const definitions: Definitions = {
  scores: ["notes", "words"],
  points: [
    { on: "note.created", score: "notes", delta: 1 },
    { on: "note.saved", score: "words", delta: { path: "payload.words" } },
  ],
  streaks: [
    // Daily writing streak. The app emits 'day.active' (idempotent per calendar day) on
    // save, and 'day.missed' on startup when a day was skipped.
    { code: "daily", tickEvents: ["day.active"], resetEvents: ["day.missed"], scoping: "per-actor" },
  ],
  tiers: [
    {
      code: "writer",
      score: "notes",
      thresholds: [
        { name: "novice", at: 1 },
        { name: "scribe", at: 10 },
        { name: "author", at: 50 },
        { name: "prolific", at: 200 },
      ],
    },
  ],
  achievements: [
    {
      code: "first_note",
      name: "Hello, World",
      description: "Created your first note",
      rarity: 1,
      metadata: { emoji: "🌱" },
      rule: { kind: "count", eventType: "note.created", gte: 1 },
    },
    {
      code: "centurion",
      name: "Centurion",
      description: "Created 100 notes",
      rarity: 4,
      metadata: { emoji: "💯" },
      rule: { kind: "count", eventType: "note.created", gte: 100 },
    },
    {
      code: "wordsmith",
      name: "Wordsmith",
      description: "Wrote 10,000 words",
      rarity: 3,
      metadata: { emoji: "✒️" },
      rule: { kind: "score", score: "words", gte: 10_000 },
    },
    {
      code: "polymath",
      name: "Polymath",
      description: "Created a note of every kind",
      rarity: 3,
      metadata: { emoji: "🧩" },
      rule: { kind: "unique", eventType: "note.created", by: "payload.kind", gte: 4 },
    },
    {
      code: "kanban_fan",
      name: "Board Game",
      description: "Created 5 kanban notes",
      rarity: 2,
      metadata: { emoji: "🗂️" },
      rule: {
        kind: "count",
        eventType: "note.created",
        gte: 5,
        where: { path: "payload.kind", op: "=", value: "kanban" },
      },
    },
    {
      code: "night_owl",
      name: "After Dark",
      description: "Saved a note between midnight and 4am",
      rarity: 2,
      metadata: { emoji: "🌙" },
      // gamekit 0.2: any event in a time-of-day window (not just the day's first).
      rule: { kind: "count", eventType: "note.saved", gte: 1, todBetween: [0, 4 * HOUR] },
    },
    {
      code: "week_warrior",
      name: "Week Warrior",
      description: "Wrote on 7 consecutive days",
      rarity: 4,
      metadata: { emoji: "🔥" },
      rule: { kind: "streak", streak: "daily", gte: 7 },
    },
  ],
};
