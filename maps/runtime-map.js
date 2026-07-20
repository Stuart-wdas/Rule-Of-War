import {
  createHexNavigation,
  createNavigationGraph,
  createSquareNavigation,
} from "../core/navigation-graph.js";

// Rendering chunks are deliberately much larger than navigation buckets.
// A 1000 x 1000 square map therefore has only 256 render chunks, while the
// simulation can continue addressing individual tiles on demand.
export const DEFAULT_RENDER_CHUNK_SIZE = 128;

function centroid(vertices) {
  return vertices.reduce(
    (center, vertex) => ({
      x: center.x + vertex.x / vertices.length,
      y: center.y + vertex.y / vertices.length,
    }),
    { x: 0, y: 0 },
  );
}

function coordinateFromKey(key) {
  const [x, y] = String(key).split(",").map(Number);
  return { x, y };
}

function fitTileWindow(bounds, maximumTiles) {
  const minX = Math.ceil(bounds.minX);
  const maxX = Math.floor(bounds.maxX);
  const minY = Math.ceil(bounds.minY);
  const maxY = Math.floor(bounds.maxY);
  const width = Math.max(0, maxX - minX + 1);
  const height = Math.max(0, maxY - minY + 1);
  if (!Number.isFinite(maximumTiles) || width * height <= maximumTiles)
    return { minX, maxX, minY, maxY };

  const aspect = Math.max(0.25, Math.min(4, width / Math.max(height, 1)));
  const fittedWidth = Math.max(1, Math.floor(Math.sqrt(maximumTiles * aspect)));
  const fittedHeight = Math.max(1, Math.floor(maximumTiles / fittedWidth));
  const centerX = (minX + maxX) / 2;
  const centerY = (minY + maxY) / 2;
  return {
    minX: Math.round(centerX - fittedWidth / 2),
    maxX: Math.round(centerX - fittedWidth / 2) + fittedWidth - 1,
    minY: Math.round(centerY - fittedHeight / 2),
    maxY: Math.round(centerY - fittedHeight / 2) + fittedHeight - 1,
  };
}

function createSquareRenderChunks(cols, rows, chunkSize) {
  const chunks = [];
  const chunkCols = Math.ceil(cols / chunkSize);
  const chunkRows = Math.ceil(rows / chunkSize);
  for (let chunkY = 0; chunkY < chunkRows; chunkY += 1) {
    for (let chunkX = 0; chunkX < chunkCols; chunkX += 1) {
      const minX = chunkX * chunkSize;
      const minY = chunkY * chunkSize;
      const maxX = Math.min(cols - 1, minX + chunkSize - 1);
      const maxY = Math.min(rows - 1, minY + chunkSize - 1);
      chunks.push({
        id: `${chunkX},${chunkY}`,
        chunkX,
        chunkY,
        minX,
        maxX,
        minY,
        maxY,
        tileCount: (maxX - minX + 1) * (maxY - minY + 1),
      });
    }
  }
  return { chunks, chunkCols, chunkRows };
}

function chunkWindowAround(
  center,
  maximumTiles,
  aspect,
  chunkSize,
  chunkCols,
  chunkRows,
) {
  const finiteMaximum = Number.isFinite(maximumTiles)
    ? Math.max(chunkSize * chunkSize, maximumTiles)
    : chunkCols * chunkRows * chunkSize * chunkSize;
  const maximumChunks = Math.max(
    1,
    Math.floor(finiteMaximum / (chunkSize * chunkSize)),
  );
  const fittedAspect = Math.max(0.25, Math.min(4, aspect || 1.6));
  let widthInChunks = Math.max(
    1,
    Math.floor(Math.sqrt(maximumChunks * fittedAspect)),
  );
  let heightInChunks = Math.max(1, Math.floor(maximumChunks / widthInChunks));
  widthInChunks = Math.min(chunkCols, widthInChunks);
  heightInChunks = Math.min(chunkRows, heightInChunks);

  const centerChunkX = Math.floor(center.x / chunkSize);
  const centerChunkY = Math.floor(center.y / chunkSize);
  const firstChunkX = Math.max(
    0,
    Math.min(
      chunkCols - widthInChunks,
      centerChunkX - Math.floor(widthInChunks / 2),
    ),
  );
  const firstChunkY = Math.max(
    0,
    Math.min(
      chunkRows - heightInChunks,
      centerChunkY - Math.floor(heightInChunks / 2),
    ),
  );
  return {
    minX: firstChunkX * chunkSize,
    maxX: Math.min(
      chunkCols * chunkSize - 1,
      (firstChunkX + widthInChunks) * chunkSize - 1,
    ),
    minY: firstChunkY * chunkSize,
    maxY: Math.min(
      chunkRows * chunkSize - 1,
      (firstChunkY + heightInChunks) * chunkSize - 1,
    ),
  };
}

function createVirtualSquareRuntimeMap(preset) {
  const { cols, rows } = preset;
  const renderChunkSize = Math.max(
    8,
    Math.floor(preset.renderChunkSize ?? DEFAULT_RENDER_CHUNK_SIZE),
  );
  const {
    chunks: renderChunks,
    chunkCols,
    chunkRows,
  } = createSquareRenderChunks(cols, rows, renderChunkSize);
  const graph = createSquareNavigation(cols, rows);
  const bounds = { minX: -0.5, maxX: cols - 0.5, minY: -0.5, maxY: rows - 0.5 };
  const getTile = (x, y) => graph.resolve({ x, y });
  const tileByCoordinate = {
    get(key) {
      const { x, y } = coordinateFromKey(key);
      return getTile(x, y);
    },
  };
  const tileIndexByCoordinate = {
    get(key) {
      const { x, y } = coordinateFromKey(key);
      return getTile(x, y) ? y * cols + x : undefined;
    },
  };
  const tilesWithinRadius = (
    origin,
    radius,
    { includeOrigin = false } = {},
  ) => {
    const center = getTile(origin.x, origin.y)?.center ?? origin;
    const result = [];
    const minX = Math.max(0, Math.ceil(center.x - radius));
    const maxX = Math.min(cols - 1, Math.floor(center.x + radius));
    const minY = Math.max(0, Math.ceil(center.y - radius));
    const maxY = Math.min(rows - 1, Math.floor(center.y + radius));
    for (let y = minY; y <= maxY; y += 1) {
      for (let x = minX; x <= maxX; x += 1) {
        const tileDistance = Math.hypot(x - center.x, y - center.y);
        if (tileDistance <= radius && (includeOrigin || tileDistance > 0))
          result.push(getTile(x, y));
      }
    }
    return result;
  };
  const tilesInBounds = (requestedBounds = bounds, maximumTiles = Infinity) => {
    const fitted = fitTileWindow(requestedBounds, maximumTiles);
    const minX = Math.max(0, fitted.minX);
    const maxX = Math.min(cols - 1, fitted.maxX);
    const minY = Math.max(0, fitted.minY);
    const maxY = Math.min(rows - 1, fitted.maxY);
    const result = [];
    for (let y = minY; y <= maxY; y += 1)
      for (let x = minX; x <= maxX; x += 1) result.push(getTile(x, y));
    return result;
  };
  const windowAround = (center, maximumTiles, aspect = 1.6) => {
    return chunkWindowAround(
      center,
      maximumTiles,
      aspect,
      renderChunkSize,
      chunkCols,
      chunkRows,
    );
  };
  const chunksInBounds = (requestedBounds = bounds) => {
    const minimumChunkX = Math.max(
      0,
      Math.floor(Math.max(0, requestedBounds.minX) / renderChunkSize),
    );
    const maximumChunkX = Math.min(
      chunkCols - 1,
      Math.floor(Math.min(cols - 1, requestedBounds.maxX) / renderChunkSize),
    );
    const minimumChunkY = Math.max(
      0,
      Math.floor(Math.max(0, requestedBounds.minY) / renderChunkSize),
    );
    const maximumChunkY = Math.min(
      chunkRows - 1,
      Math.floor(Math.min(rows - 1, requestedBounds.maxY) / renderChunkSize),
    );
    const result = [];
    for (let chunkY = minimumChunkY; chunkY <= maximumChunkY; chunkY += 1)
      for (let chunkX = minimumChunkX; chunkX <= maximumChunkX; chunkX += 1)
        result.push(renderChunks[chunkY * chunkCols + chunkX]);
    return result;
  };
  const sampleTiles = (maximumTiles = 4096) => {
    if (cols * rows <= maximumTiles) return tilesInBounds(bounds);
    const stride = Math.max(
      1,
      Math.ceil(Math.sqrt((cols * rows) / maximumTiles)),
    );
    const result = [];
    for (let y = 0; y < rows && result.length < maximumTiles; y += stride)
      for (let x = 0; x < cols && result.length < maximumTiles; x += stride)
        result.push(getTile(x, y));
    return result;
  };

  return {
    id: preset.id,
    name: preset.name,
    polygonal: false,
    layout: preset.layout ?? "square",
    virtual: true,
    tileCount: cols * rows,
    cols,
    rows,
    // Authored polygon maps retain a materialized tile array. Regular square
    // maps deliberately do not; use the bounded query methods below.
    tiles: [],
    getTile,
    tileByCoordinate,
    tileIndexByCoordinate,
    graph,
    bounds,
    spatialBuckets: null,
    renderChunkSize,
    renderChunks,
    chunksInBounds,
    tilesWithinRadius,
    tilesInBounds,
    windowAround,
    sampleTiles,
  };
}

export function createRuntimeMap(preset) {
  const sourceTiles = preset.tiles ?? preset.createTiles?.() ?? null;
  if (!sourceTiles) return createVirtualSquareRuntimeMap(preset);
  const graph = sourceTiles
    ? preset.layout === "hex-odd-q"
      ? createHexNavigation(sourceTiles)
      : createNavigationGraph(sourceTiles)
    : createSquareNavigation(preset.cols, preset.rows);
  const tiles = graph
    .allTiles()
    .map((tile) => ({
      ...tile,
      center: tile.center ?? centroid(tile.vertices),
    }));
  const bounds = {
    minX: Number.POSITIVE_INFINITY,
    maxX: Number.NEGATIVE_INFINITY,
    minY: Number.POSITIVE_INFINITY,
    maxY: Number.NEGATIVE_INFINITY,
  };
  for (const tile of tiles) {
    for (const vertex of tile.vertices) {
      bounds.minX = Math.min(bounds.minX, vertex.x);
      bounds.maxX = Math.max(bounds.maxX, vertex.x);
      bounds.minY = Math.min(bounds.minY, vertex.y);
      bounds.maxY = Math.max(bounds.maxY, vertex.y);
    }
  }
  const tileByCoordinate = new Map(
    tiles.map((tile) => [`${tile.x},${tile.y}`, tile]),
  );
  const tileIndexByCoordinate = new Map(
    tiles.map((tile, index) => [`${tile.x},${tile.y}`, index]),
  );
  // Uniform buckets make circular range, fog, and AI queries scale with the
  // searched area instead of the total number of map tiles.
  const bucketSize = 8;
  const spatialBuckets = new Map();
  for (const tile of tiles) {
    const bucketX = Math.floor(tile.center.x / bucketSize);
    const bucketY = Math.floor(tile.center.y / bucketSize);
    const key = `${bucketX},${bucketY}`;
    const bucket = spatialBuckets.get(key) ?? [];
    bucket.push(tile);
    spatialBuckets.set(key, bucket);
  }
  const tilesWithinRadius = (
    origin,
    radius,
    { includeOrigin = false } = {},
  ) => {
    const center =
      tileByCoordinate.get(`${origin.x},${origin.y}`)?.center ?? origin;
    const result = [];
    const minBucketX = Math.floor((center.x - radius) / bucketSize);
    const maxBucketX = Math.floor((center.x + radius) / bucketSize);
    const minBucketY = Math.floor((center.y - radius) / bucketSize);
    const maxBucketY = Math.floor((center.y + radius) / bucketSize);
    for (let bucketY = minBucketY; bucketY <= maxBucketY; bucketY += 1) {
      for (let bucketX = minBucketX; bucketX <= maxBucketX; bucketX += 1) {
        for (const tile of spatialBuckets.get(`${bucketX},${bucketY}`) ?? []) {
          const tileDistance = Math.hypot(
            tile.center.x - center.x,
            tile.center.y - center.y,
          );
          if (tileDistance <= radius && (includeOrigin || tileDistance > 0))
            result.push(tile);
        }
      }
    }
    return result;
  };
  return {
    id: preset.id,
    name: preset.name,
    polygonal: Boolean(sourceTiles),
    layout: preset.layout ?? "polygon",
    virtual: false,
    tileCount: tiles.length,
    cols: preset.cols,
    rows: preset.rows,
    tiles,
    tileByCoordinate,
    tileIndexByCoordinate,
    graph,
    bounds,
    spatialBuckets,
    tilesWithinRadius,
    tilesInBounds(requestedBounds = bounds, maximumTiles = Infinity) {
      const candidates = tiles.filter(
        (tile) =>
          tile.center.x >= requestedBounds.minX &&
          tile.center.x <= requestedBounds.maxX &&
          tile.center.y >= requestedBounds.minY &&
          tile.center.y <= requestedBounds.maxY,
      );
      return candidates.slice(0, maximumTiles);
    },
    windowAround(center, maximumTiles, aspect = 1.6) {
      const width = Math.max(1, Math.sqrt(maximumTiles * aspect));
      const height = Math.max(1, maximumTiles / width);
      return {
        minX: center.x - width / 2,
        maxX: center.x + width / 2,
        minY: center.y - height / 2,
        maxY: center.y + height / 2,
      };
    },
    sampleTiles(maximumTiles = 4096) {
      if (tiles.length <= maximumTiles) return [...tiles];
      const stride = Math.ceil(tiles.length / maximumTiles);
      return tiles
        .filter((_, index) => index % stride === 0)
        .slice(0, maximumTiles);
    },
  };
}

export function deploymentTiles(runtimeMap, owner, limit = 256) {
  if (runtimeMap.virtual) {
    const middle = (runtimeMap.rows - 1) / 2;
    const yOrder = Array.from({ length: runtimeMap.rows }, (_, y) => y).sort(
      (a, b) => Math.abs(a - middle) - Math.abs(b - middle) || a - b,
    );
    const result = [];
    for (
      let offset = 0;
      offset < runtimeMap.cols && result.length < limit;
      offset += 1
    ) {
      const x = owner === 0 ? offset : runtimeMap.cols - 1 - offset;
      for (const y of yOrder) {
        result.push(runtimeMap.getTile(x, y));
        if (result.length >= limit) break;
      }
    }
    return result;
  }
  const centerY = (runtimeMap.bounds.minY + runtimeMap.bounds.maxY) / 2;
  return [...runtimeMap.tiles]
    .sort((a, b) => {
      const edgeOrder =
        owner === 0 ? a.center.x - b.center.x : b.center.x - a.center.x;
      return (
        edgeOrder ||
        Math.abs(a.center.y - centerY) - Math.abs(b.center.y - centerY) ||
        String(a.id).localeCompare(String(b.id))
      );
    })
    .slice(0, limit);
}
