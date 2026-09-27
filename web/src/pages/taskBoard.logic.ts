// Pure, dependency-free task-event merge logic for the Task Board live stream.
//
// The gateway replays a bounded window of task events on every (re)connect, so
// the client must dedupe by event id and keep the buffer bounded rather than
// assuming it only ever receives each event once.

export interface TaskEvent {
  id: number;
  event_type: string;
  payload: unknown;
  timestamp: string;
}

/** Max events retained in the detail activity feed. */
export const MAX_TASK_EVENTS = 100;

/**
 * Merge incoming events into the existing feed: drop duplicates by id, sort by
 * id ascending, and keep only the most recent `cap` events.
 */
export function mergeTaskEvents(
  prev: TaskEvent[],
  incoming: TaskEvent[],
  cap: number = MAX_TASK_EVENTS,
): TaskEvent[] {
  const byId = new Map<number, TaskEvent>();
  for (const event of prev) byId.set(event.id, event);
  for (const event of incoming) byId.set(event.id, event);
  const merged = [...byId.values()].sort((a, b) => a.id - b.id);
  return merged.length > cap ? merged.slice(merged.length - cap) : merged;
}

/** True when an SSE frame carries a task event (vs. an error/comment frame). */
export function isTaskEventFrame(frame: { type?: string }): boolean {
  return frame.type === 'task_event';
}

/** Minimal shape needed to reconstruct a parent-child tree. */
export interface TaskTreeNodeRecord {
  id: string;
  parent_id: string | null;
}

export interface TaskTreeNode<T> {
  record: T;
  children: TaskTreeNode<T>[];
}

/**
 * Rebuild the descendant tree under `rootId` from a flat descendant list (the
 * gateway `/tree` projection). Children are ordered by id for a stable view.
 */
export function buildTaskTree<T extends TaskTreeNodeRecord>(
  rootId: string,
  records: T[],
): TaskTreeNode<T>[] {
  const byParent = new Map<string, T[]>();
  for (const record of records) {
    const key = record.parent_id ?? '';
    const siblings = byParent.get(key);
    if (siblings) siblings.push(record);
    else byParent.set(key, [record]);
  }
  const build = (parentId: string): TaskTreeNode<T>[] =>
    (byParent.get(parentId) ?? [])
      .slice()
      .sort((a, b) => a.id.localeCompare(b.id))
      .map((record) => ({ record, children: build(record.id) }));
  return build(rootId);
}
