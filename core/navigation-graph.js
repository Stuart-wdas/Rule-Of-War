const DEFAULT_EPSILON = 1e-6;

const coordinateKey = (x, y) => `${x},${y}`;
const quantize = (value, epsilon) => Math.round(value / epsilon);

function edgeRecord(tile, start, end, epsilon) {
  let dx = end.x - start.x;
  let dy = end.y - start.y;
  const length = Math.hypot(dx, dy);
  if (length <= epsilon) return null;
  dx /= length;
  dy /= length;
  if (dx < -epsilon || (Math.abs(dx) <= epsilon && dy < 0)) {
    dx *= -1;
    dy *= -1;
  }
  const normalX = -dy;
  const normalY = dx;
  const first = dx * start.x + dy * start.y;
  const second = dx * end.x + dy * end.y;
  return {
    tile,
    start: Math.min(first, second),
    end: Math.max(first, second),
    lineKey: `${quantize(dx, epsilon)}:${quantize(dy, epsilon)}:${quantize(normalX * start.x + normalY * start.y, epsilon)}`,
  };
}

/**
 * Connects polygons whose boundaries share a positive-length edge segment.
 * Partial edge overlaps count, so different tilable shapes may be mixed.
 */
export class NavigationGraph {
  constructor(tiles, { epsilon = DEFAULT_EPSILON } = {}) {
    this.tileById = new Map();
    this.tileByCoordinate = new Map();
    this.adjacency = new Map();
    const edgeLines = new Map();

    for (const source of tiles) {
      if (!Array.isArray(source.vertices) || source.vertices.length < 3)
        throw new TypeError("Every navigation tile needs at least three vertices.");
      const tile = { ...source, id: source.id ?? coordinateKey(source.x, source.y) };
      const positionKey = coordinateKey(tile.x, tile.y);
      if (this.tileById.has(tile.id)) throw new Error(`Duplicate navigation tile id: ${tile.id}`);
      if (this.tileByCoordinate.has(positionKey)) throw new Error(`Duplicate navigation coordinate: ${positionKey}`);
      this.tileById.set(tile.id, tile);
      this.tileByCoordinate.set(positionKey, tile);
      this.adjacency.set(tile.id, new Set());

      for (let index = 0; index < tile.vertices.length; index += 1) {
        const edge = edgeRecord(tile, tile.vertices[index], tile.vertices[(index + 1) % tile.vertices.length], epsilon);
        if (!edge) continue;
        const line = edgeLines.get(edge.lineKey) ?? [];
        line.push(edge);
        edgeLines.set(edge.lineKey, line);
      }
    }

    for (const edges of edgeLines.values()) {
      for (let left = 0; left < edges.length; left += 1) {
        for (let right = left + 1; right < edges.length; right += 1) {
          const a = edges[left];
          const b = edges[right];
          const overlap = Math.min(a.end, b.end) - Math.max(a.start, b.start);
          if (a.tile.id === b.tile.id || overlap <= epsilon) continue;
          this.adjacency.get(a.tile.id).add(b.tile.id);
          this.adjacency.get(b.tile.id).add(a.tile.id);
        }
      }
    }

    this.componentByTile = new Map();
    let component = 0;
    for (const tileId of this.tileById.keys()) {
      if (this.componentByTile.has(tileId)) continue;
      const queue = [tileId];
      this.componentByTile.set(tileId, component);
      for (let cursor = 0; cursor < queue.length; cursor += 1) {
        for (const neighborId of this.adjacency.get(queue[cursor])) {
          if (this.componentByTile.has(neighborId)) continue;
          this.componentByTile.set(neighborId, component);
          queue.push(neighborId);
        }
      }
      component += 1;
    }
  }

  resolve(tileOrPoint) {
    if (!tileOrPoint) return null;
    if (tileOrPoint.id != null && this.tileById.has(tileOrPoint.id)) return this.tileById.get(tileOrPoint.id);
    return this.tileByCoordinate.get(coordinateKey(tileOrPoint.x, tileOrPoint.y)) ?? null;
  }

  allTiles() {
    return [...this.tileById.values()];
  }

  neighbors(tileOrPoint) {
    const tile = this.resolve(tileOrPoint);
    if (!tile) return [];
    return [...this.adjacency.get(tile.id)].map((id) => this.tileById.get(id));
  }

  /** True when an unbroken chain of shared edges joins both tiles. */
  isConnected(origin, destination) {
    const start = this.resolve(origin);
    const target = this.resolve(destination);
    return Boolean(
      start &&
        target &&
        this.componentByTile.get(start.id) ===
          this.componentByTile.get(target.id),
    );
  }

  reachable(origin, maximumSteps, { includeOrigin = false } = {}) {
    const start = this.resolve(origin);
    if (!start || !Number.isInteger(maximumSteps) || maximumSteps < 0) return [];
    const steps = new Map([[start.id, 0]]);
    const queue = [start];
    for (let cursor = 0; cursor < queue.length; cursor += 1) {
      const current = queue[cursor];
      if (steps.get(current.id) >= maximumSteps) continue;
      for (const neighborId of this.adjacency.get(current.id)) {
        if (steps.has(neighborId)) continue;
        steps.set(neighborId, steps.get(current.id) + 1);
        queue.push(this.tileById.get(neighborId));
      }
    }
    return queue.filter((tile) => includeOrigin || tile.id !== start.id);
  }

  stepsBetween(origin, destination, maximumSteps = Number.POSITIVE_INFINITY) {
    const start = this.resolve(origin);
    const target = this.resolve(destination);
    if (!start || !target) return null;
    if (start.id === target.id) return 0;
    const steps = new Map([[start.id, 0]]);
    const queue = [start.id];
    for (let cursor = 0; cursor < queue.length; cursor += 1) {
      const currentId = queue[cursor];
      const nextSteps = steps.get(currentId) + 1;
      if (nextSteps > maximumSteps) continue;
      for (const neighborId of this.adjacency.get(currentId)) {
        if (steps.has(neighborId)) continue;
        if (neighborId === target.id) return nextSteps;
        steps.set(neighborId, nextSteps);
        queue.push(neighborId);
      }
    }
    return null;
  }

  canReach(origin, destination, maximumSteps) {
    const steps = this.stepsBetween(origin, destination, maximumSteps);
    return steps !== null && steps > 0 && steps <= maximumSteps;
  }
}

export function createNavigationGraph(tiles, options) {
  return new NavigationGraph(tiles, options);
}

/**
 * Fast graph for regular grids whose neighbours are known mathematically.
 * It avoids edge quantization and shared-edge comparison on very large maps.
 */
class RegularNavigationGraph {
  constructor(tiles, neighborCoordinates) {
    this.tileById = new Map();
    this.tileByCoordinate = new Map();
    this.adjacency = new Map();
    for (const tile of tiles) {
      this.tileById.set(tile.id, tile);
      this.tileByCoordinate.set(coordinateKey(tile.x, tile.y), tile);
    }
    for (const tile of tiles) {
      const neighbors = new Set();
      for (const point of neighborCoordinates(tile)) {
        const neighbor = this.tileByCoordinate.get(
          coordinateKey(point.x, point.y),
        );
        if (neighbor) neighbors.add(neighbor.id);
      }
      this.adjacency.set(tile.id, neighbors);
    }
  }

  resolve(tileOrPoint) {
    if (!tileOrPoint) return null;
    if (tileOrPoint.id != null && this.tileById.has(tileOrPoint.id))
      return this.tileById.get(tileOrPoint.id);
    return (
      this.tileByCoordinate.get(
        coordinateKey(tileOrPoint.x, tileOrPoint.y),
      ) ?? null
    );
  }

  allTiles() {
    return [...this.tileById.values()];
  }

  neighbors(tileOrPoint) {
    const tile = this.resolve(tileOrPoint);
    if (!tile) return [];
    return [...this.adjacency.get(tile.id)].map((id) => this.tileById.get(id));
  }

  isConnected(origin, destination) {
    return Boolean(this.resolve(origin) && this.resolve(destination));
  }

  reachable(origin, maximumSteps, { includeOrigin = false } = {}) {
    const start = this.resolve(origin);
    if (!start || !Number.isInteger(maximumSteps) || maximumSteps < 0) return [];
    const steps = new Map([[start.id, 0]]);
    const queue = [start];
    for (let cursor = 0; cursor < queue.length; cursor += 1) {
      const current = queue[cursor];
      if (steps.get(current.id) >= maximumSteps) continue;
      for (const neighborId of this.adjacency.get(current.id)) {
        if (steps.has(neighborId)) continue;
        steps.set(neighborId, steps.get(current.id) + 1);
        queue.push(this.tileById.get(neighborId));
      }
    }
    return queue.filter((tile) => includeOrigin || tile.id !== start.id);
  }

  stepsBetween(origin, destination, maximumSteps = Number.POSITIVE_INFINITY) {
    const start = this.resolve(origin);
    const target = this.resolve(destination);
    if (!start || !target) return null;
    if (start.id === target.id) return 0;
    const steps = new Map([[start.id, 0]]);
    const queue = [start.id];
    for (let cursor = 0; cursor < queue.length; cursor += 1) {
      const currentId = queue[cursor];
      const nextSteps = steps.get(currentId) + 1;
      if (nextSteps > maximumSteps) continue;
      for (const neighborId of this.adjacency.get(currentId)) {
        if (steps.has(neighborId)) continue;
        if (neighborId === target.id) return nextSteps;
        steps.set(neighborId, nextSteps);
        queue.push(neighborId);
      }
    }
    return null;
  }

  canReach(origin, destination, maximumSteps) {
    const steps = this.stepsBetween(origin, destination, maximumSteps);
    return steps !== null && steps > 0 && steps <= maximumSteps;
  }
}

/**
 * Mathematical square-grid graph.
 *
 * A regular 1000 x 1000 map does not need one million tile, edge, and
 * adjacency objects in memory. Tiles are lightweight values produced only
 * when a rule, renderer window, or path query asks for them.
 */
class VirtualSquareNavigationGraph {
  constructor(cols, rows) {
    this.cols = cols;
    this.rows = rows;
    this.virtual = true;
  }

  resolve(tileOrPoint) {
    if (!tileOrPoint) return null;
    let x = Number(tileOrPoint.x);
    let y = Number(tileOrPoint.y);
    if ((!Number.isInteger(x) || !Number.isInteger(y)) && tileOrPoint.id) {
      const [idX, idY] = String(tileOrPoint.id).split(",").map(Number);
      x = idX;
      y = idY;
    }
    if (
      !Number.isInteger(x) ||
      !Number.isInteger(y) ||
      x < 0 ||
      y < 0 ||
      x >= this.cols ||
      y >= this.rows
    )
      return null;
    return {
      id: coordinateKey(x, y),
      x,
      y,
      center: { x, y },
      vertices: [
        { x: x - 0.5, y: y - 0.5 },
        { x: x + 0.5, y: y - 0.5 },
        { x: x + 0.5, y: y + 0.5 },
        { x: x - 0.5, y: y + 0.5 },
      ],
    };
  }

  allTiles(maximum = Number.POSITIVE_INFINITY) {
    const limit = Math.min(this.cols * this.rows, maximum);
    const tiles = [];
    for (let index = 0; index < limit; index += 1) {
      const x = index % this.cols;
      const y = Math.floor(index / this.cols);
      tiles.push(this.resolve({ x, y }));
    }
    return tiles;
  }

  neighbors(tileOrPoint) {
    const tile = this.resolve(tileOrPoint);
    if (!tile) return [];
    return [
      this.resolve({ x: tile.x - 1, y: tile.y }),
      this.resolve({ x: tile.x + 1, y: tile.y }),
      this.resolve({ x: tile.x, y: tile.y - 1 }),
      this.resolve({ x: tile.x, y: tile.y + 1 }),
    ].filter(Boolean);
  }

  isConnected(origin, destination) {
    return Boolean(this.resolve(origin) && this.resolve(destination));
  }

  reachable(origin, maximumSteps, { includeOrigin = false } = {}) {
    const start = this.resolve(origin);
    if (!start || !Number.isInteger(maximumSteps) || maximumSteps < 0)
      return [];
    const tiles = [];
    for (let offsetY = -maximumSteps; offsetY <= maximumSteps; offsetY += 1) {
      const remaining = maximumSteps - Math.abs(offsetY);
      for (let offsetX = -remaining; offsetX <= remaining; offsetX += 1) {
        if (!includeOrigin && offsetX === 0 && offsetY === 0) continue;
        const tile = this.resolve({
          x: start.x + offsetX,
          y: start.y + offsetY,
        });
        if (tile) tiles.push(tile);
      }
    }
    return tiles;
  }

  stepsBetween(origin, destination, maximumSteps = Number.POSITIVE_INFINITY) {
    const start = this.resolve(origin);
    const target = this.resolve(destination);
    if (!start || !target) return null;
    const steps = Math.abs(target.x - start.x) + Math.abs(target.y - start.y);
    return steps <= maximumSteps ? steps : null;
  }

  canReach(origin, destination, maximumSteps) {
    const steps = this.stepsBetween(origin, destination, maximumSteps);
    return steps !== null && steps > 0 && steps <= maximumSteps;
  }
}

/** Default square board: shared sides connect, diagonal corners do not. */
export function createSquareNavigation(cols, rows) {
  return new VirtualSquareNavigationGraph(cols, rows);
}

/** Pointy-top, odd-column-offset hex grid used by the honeycomb preset. */
export function createHexNavigation(tiles) {
  return new RegularNavigationGraph(tiles, ({ x, y }) => {
    const diagonalRows = x % 2 === 0 ? [y - 1, y] : [y, y + 1];
    return [
      { x, y: y - 1 },
      { x, y: y + 1 },
      { x: x - 1, y: diagonalRows[0] },
      { x: x - 1, y: diagonalRows[1] },
      { x: x + 1, y: diagonalRows[0] },
      { x: x + 1, y: diagonalRows[1] },
    ];
  });
}
