class MinimumHeap {
  constructor() {
    this.items = [];
  }

  push(value) {
    this.items.push(value);
    let index = this.items.length - 1;
    while (index > 0) {
      const parent = Math.floor((index - 1) / 2);
      if (this.items[parent].priority <= value.priority) break;
      this.items[index] = this.items[parent];
      index = parent;
    }
    this.items[index] = value;
  }

  pop() {
    if (!this.items.length) return null;
    const first = this.items[0];
    const last = this.items.pop();
    if (!this.items.length) return first;
    let index = 0;
    while (true) {
      const left = index * 2 + 1;
      const right = left + 1;
      if (left >= this.items.length) break;
      const child =
        right < this.items.length &&
        this.items[right].priority < this.items[left].priority
          ? right
          : left;
      if (this.items[child].priority >= last.priority) break;
      this.items[index] = this.items[child];
      index = child;
    }
    this.items[index] = last;
    return first;
  }
}

const keyOf = (cell) => `${cell.x},${cell.y}`;

/** Finds the lowest movement-cost route without changing map topology. */
export function findTerrainPath({
  graph,
  start,
  goal,
  movementCostAt,
  distance,
  maximumVisited = Number.POSITIVE_INFINITY,
}) {
  const origin = graph.resolve(start);
  const destination = graph.resolve(goal);
  if (!origin || !destination) return null;
  const originKey = keyOf(origin);
  const destinationKey = keyOf(destination);
  const frontier = new MinimumHeap();
  const cost = new Map([[originKey, 0]]);
  const previous = new Map();
  const cells = new Map([[originKey, origin]]);
  frontier.push({ cell: origin, priority: 0, cost: 0 });
  let visited = 0;

  while (frontier.items.length && visited < maximumVisited) {
    const currentEntry = frontier.pop();
    const current = currentEntry.cell;
    const currentKey = keyOf(current);
    if (currentEntry.cost !== cost.get(currentKey)) continue;
    visited += 1;
    if (currentKey === destinationKey) break;

    for (const neighbour of graph.neighbors(current)) {
      const traversalCost = movementCostAt(neighbour);
      if (!Number.isFinite(traversalCost)) continue;
      const stepCost = Math.max(distance(current, neighbour), 1e-6) * traversalCost;
      const nextCost = currentEntry.cost + stepCost;
      const neighbourKey = keyOf(neighbour);
      if (nextCost >= (cost.get(neighbourKey) ?? Number.POSITIVE_INFINITY))
        continue;
      cost.set(neighbourKey, nextCost);
      previous.set(neighbourKey, currentKey);
      cells.set(neighbourKey, neighbour);
      frontier.push({
        cell: neighbour,
        cost: nextCost,
        priority: nextCost + distance(neighbour, destination),
      });
    }
  }

  if (!cost.has(destinationKey)) return null;
  const path = [];
  let cursor = destinationKey;
  while (cursor) {
    const cell = cells.get(cursor) ?? (cursor === originKey ? origin : null);
    if (!cell) return null;
    path.push(cell);
    if (cursor === originKey) break;
    cursor = previous.get(cursor);
  }
  path.reverse();
  return { path, cost: cost.get(destinationKey), visited };
}
