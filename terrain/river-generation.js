import { createSeededRandom, deriveSeed } from "./seeded-random.js";

const NEIGHBOUR_OFFSETS = Object.freeze([
  [-1, -1],
  [0, -1],
  [1, -1],
  [-1, 0],
  [1, 0],
  [-1, 1],
  [0, 1],
  [1, 1],
]);

function isBoundary(x, y, width, height) {
  return x === 0 || y === 0 || x === width - 1 || y === height - 1;
}

function addFlowDownstream(start, amount, river, flow, downstream) {
  const visited = new Set();
  let current = start;
  while (current >= 0 && river[current] && !visited.has(current)) {
    visited.add(current);
    flow[current] += amount;
    current = downstream[current];
  }
}

function sourceCells(elevation, moisture, width, height, seed, config) {
  const random = createSeededRandom(deriveSeed(seed, "sources"));
  const candidates = [];
  for (let y = 1; y < height - 1; y += 1) {
    for (let x = 1; x < width - 1; x += 1) {
      const index = y * width + x;
      if (elevation[index] < config.sourceElevation) continue;
      candidates.push({
        index,
        x,
        y,
        score:
          elevation[index] * 0.62 + moisture[index] * 0.25 + random() * 0.13,
      });
    }
  }
  candidates.sort((first, second) => second.score - first.score);
  const desired = Math.max(
    config.minimumRiverCount,
    Math.min(
      config.maximumRiverCount,
      Math.round(width * height * config.sourceDensity),
    ),
  );
  const chosen = [];
  const spacingSquared = config.minimumSourceSpacing ** 2;
  for (const candidate of candidates) {
    if (
      chosen.every(
        (source) =>
          (source.x - candidate.x) ** 2 + (source.y - candidate.y) ** 2 >=
          spacingSquared,
      )
    )
      chosen.push(candidate);
    if (chosen.length >= desired) break;
  }
  return chosen;
}

/**
 * Rivers use the terrain raster only; the game's square/hex navigation graph
 * remains unchanged. Each step examines all eight neighbours and chooses the
 * lowest. When no downhill exit exists, a tiny deterministic breach is cut in
 * the least-high neighbour. Existing river cells terminate the new trace and
 * receive its flow all the way downstream, which creates natural confluences.
 */
export function generateRivers({
  elevation,
  moisture,
  width,
  height,
  seed,
  waterLevel,
  config,
}) {
  const river = new Uint8Array(width * height);
  const flow = new Float32Array(width * height);
  const downstream = new Int32Array(width * height);
  downstream.fill(-1);
  const sources = sourceCells(
    elevation,
    moisture,
    width,
    height,
    seed,
    config,
  );
  const maximumSteps = Math.max(
    1,
    Math.min(width * height, (width + height) * config.maximumLengthFactor),
  );

  for (const source of sources) {
    if (river[source.index]) continue;
    const incomingFlow =
      config.baseFlow + moisture[source.index] * config.moistureFlow;
    const visited = new Set();
    let current = source.index;

    for (let step = 0; step < maximumSteps; step += 1) {
      if (river[current]) {
        addFlowDownstream(current, incomingFlow, river, flow, downstream);
        break;
      }
      if (elevation[current] < waterLevel || visited.has(current)) break;
      visited.add(current);
      river[current] = 1;
      flow[current] += incomingFlow;

      const currentX = current % width;
      const currentY = Math.floor(current / width);
      if (isBoundary(currentX, currentY, width, height)) break;

      const neighbours = [];
      for (const [offsetX, offsetY] of NEIGHBOUR_OFFSETS) {
        const x = currentX + offsetX;
        const y = currentY + offsetY;
        if (x < 0 || y < 0 || x >= width || y >= height) continue;
        const index = y * width + x;
        if (visited.has(index)) continue;
        neighbours.push({ index, elevation: elevation[index] });
      }
      neighbours.sort(
        (first, second) =>
          first.elevation - second.elevation || first.index - second.index,
      );
      const next = neighbours[0];
      if (!next) break;

      if (next.elevation >= elevation[current] - config.minimumDrop) {
        elevation[next.index] = Math.max(
          0,
          elevation[current] - config.depressionBreach,
        );
      }
      if (elevation[next.index] < waterLevel) break;
      downstream[current] = next.index;
      if (river[next.index]) {
        addFlowDownstream(next.index, incomingFlow, river, flow, downstream);
        break;
      }
      current = next.index;
    }
  }

  return { river, flow, downstream, sources };
}
