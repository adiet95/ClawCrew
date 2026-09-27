import assert from "node:assert/strict";
import test from "node:test";

import {
  MAX_TASK_EVENTS,
  buildTaskTree,
  isTaskEventFrame,
  mergeTaskEvents,
  type TaskEvent,
} from "./taskBoard.logic.ts";

function event(id: number): TaskEvent {
  return { id, event_type: "started", payload: { id }, timestamp: "2026-01-01T00:00:00Z" };
}

test("mergeTaskEvents appends new events in id order", () => {
  const merged = mergeTaskEvents([event(1)], [event(3), event(2)]);
  assert.deepEqual(
    merged.map((e) => e.id),
    [1, 2, 3],
  );
});

test("mergeTaskEvents dedupes replayed events by id (reconnect resync)", () => {
  const merged = mergeTaskEvents([event(1), event(2)], [event(2), event(3)]);
  assert.deepEqual(
    merged.map((e) => e.id),
    [1, 2, 3],
  );
});

test("mergeTaskEvents keeps only the most recent cap events", () => {
  const prev = Array.from({ length: MAX_TASK_EVENTS }, (_, i) => event(i));
  const merged = mergeTaskEvents(prev, [event(MAX_TASK_EVENTS + 5)]);
  assert.equal(merged.length, MAX_TASK_EVENTS);
  assert.equal(merged[merged.length - 1]!.id, MAX_TASK_EVENTS + 5);
  assert.ok(!merged.some((e) => e.id === 0));
});

test("isTaskEventFrame accepts only task_event frames", () => {
  assert.equal(isTaskEventFrame({ type: "task_event" }), true);
  assert.equal(isTaskEventFrame({ type: "error" }), false);
  assert.equal(isTaskEventFrame({}), false);
});

test("buildTaskTree nests descendants under their parent", () => {
  const records = [
    { id: "a", parent_id: "root" },
    { id: "b", parent_id: "a" },
    { id: "c", parent_id: "root" },
    { id: "unrelated", parent_id: "other" },
  ];
  const tree = buildTaskTree("root", records);
  assert.deepEqual(
    tree.map((n) => n.record.id),
    ["a", "c"],
  );
  assert.deepEqual(
    tree[0]!.children.map((n) => n.record.id),
    ["b"],
  );
  assert.equal(tree[1]!.children.length, 0);
  // Descendants of another root are not included.
  assert.ok(!tree.some((n) => n.record.id === "unrelated"));
});

test("buildTaskTree is empty for a leaf", () => {
  assert.deepEqual(buildTaskTree("leaf", []), []);
});
