import { TERRAIN_TYPES } from "./terrain-types.js";

const CARDINAL_NEIGHBOURS = Object.freeze([
  [-1, 0],
  [1, 0],
  [0, -1],
  [0, 1],
]);

const DIAGONAL_NEIGHBOURS = Object.freeze([
  ...CARDINAL_NEIGHBOURS,
  [-1, -1],
  [1, -1],
  [-1, 1],
  [1, 1],
]);

function isOpenWater(terrainType) {
  return (
    terrainType === TERRAIN_TYPES.DEEP_WATER ||
    terrainType === TERRAIN_TYPES.SHALLOW_WATER
  );
}

/**
 * Levels the rendered surface of sufficiently large connected water bodies.
 * Underlying elevation is left untouched, preserving deep/shallow terrain
 * classification and gameplay data while lakes and seas render as one plane.
 */
export function generateWaterSurfaceMap({
  elevation,
  terrainTypes,
  width,
  height,
  shallowWaterLevel,
  config,
}) {
  const surfaceElevation = new Float32Array(elevation);
  const visited = new Uint8Array(width * height);
  const neighbours = config.includeDiagonalConnections
    ? DIAGONAL_NEIGHBOURS
    : CARDINAL_NEIGHBOURS;
  const flatBodies = [];

  for (let start = 0; start < terrainTypes.length; start += 1) {
    if (visited[start] || !isOpenWater(terrainTypes[start])) continue;
    visited[start] = 1;
    const component = [start];
    let highestBed = elevation[start];
    for (let cursor = 0; cursor < component.length; cursor += 1) {
      const current = component[cursor];
      const currentX = current % width;
      const currentY = Math.floor(current / width);
      highestBed = Math.max(highestBed, elevation[current]);
      for (const [offsetX, offsetY] of neighbours) {
        const x = currentX + offsetX;
        const y = currentY + offsetY;
        if (x < 0 || y < 0 || x >= width || y >= height) continue;
        const index = y * width + x;
        if (visited[index] || !isOpenWater(terrainTypes[index])) continue;
        visited[index] = 1;
        component.push(index);
      }
    }

    if (component.length < config.minimumFlatCells) continue;
    const level = Number.isFinite(config.fixedSurfaceElevation)
      ? Math.max(0, Math.min(1, config.fixedSurfaceElevation))
      : Math.min(
          shallowWaterLevel - config.shorelineOffset,
          highestBed + config.maximumSurfaceLift,
        );
    for (const index of component) surfaceElevation[index] = level;
    flatBodies.push({ size: component.length, level });
  }

  return { surfaceElevation, flatBodies };
}
