import {
  generateElevationMap,
  generateMoistureMap,
} from "./elevation-generation.js";
import { generateRivers } from "./river-generation.js";
import { generateWaterSurfaceMap } from "./water-surface-generation.js";
import { deriveSeed, seedFrom } from "./seeded-random.js";
import {
  DEFAULT_TERRAIN_GAMEPLAY_RULES,
  terrainGameplayProperties,
} from "./terrain-gameplay-rules.js";
import {
  DEFAULT_TERRAIN_THRESHOLDS,
  TERRAIN_DEBUG_PALETTE,
  TERRAIN_PALETTE,
  TERRAIN_TYPES,
  classifyElevation,
} from "./terrain-types.js";

export const TERRAIN_MAX_ELEVATION = 10;

export const DEFAULT_TERRAIN_GENERATION_CONFIG = Object.freeze({
  maxElevation: TERRAIN_MAX_ELEVATION,
  thresholds: DEFAULT_TERRAIN_THRESHOLDS,
  // Wavelengths are measured in map tiles. Increase them for broader, slower
  // changes; reduce them for tighter terrain detail. Octaves/persistence/
  // lacunarity below control how many noise layers are combined.
  elevation: Object.freeze({
    broadWavelength: 46,
    detailWavelength: 9,
    warpWavelength: 58,
    warpStrength: 8,
    broadWeight: 0.76,
    detailWeight: 0.24,
    broadNoise: Object.freeze({
      octaves: 4,
      persistence: 0.52,
      lacunarity: 2.03,
    }),
    detailNoise: Object.freeze({
      octaves: 3,
      persistence: 0.48,
      lacunarity: 2.11,
    }),
  }),
  moisture: Object.freeze({
    broadWavelength: 34,
    detailWavelength: 8,
    warpWavelength: 51,
    warpStrength: 6,
    broadWeight: 0.7,
    detailWeight: 0.3,
    broadNoise: Object.freeze({ octaves: 4, persistence: 0.55, lacunarity: 2 }),
    detailNoise: Object.freeze({
      octaves: 3,
      persistence: 0.5,
      lacunarity: 2.07,
    }),
  }),
  waterBodies: Object.freeze({
    minimumFlatCells: 12,
    includeDiagonalConnections: true,
    shorelineOffset: 0.006,
    maximumSurfaceLift: 0.12,
    fixedSurfaceElevation: null,
  }),
  rivers: Object.freeze({
    sourceElevation: 0.68,
    sourceDensity: 0.0009,
    minimumRiverCount: 1,
    maximumRiverCount: 40,
    minimumSourceSpacing: 8,
    minimumDrop: 0.0005,
    depressionBreach: 0.0025,
    maximumLengthFactor: 6,
    baseFlow: 1,
    moistureFlow: 1.25,
  }),
  gameplay: DEFAULT_TERRAIN_GAMEPLAY_RULES,
});

function mergeConfig(overrides = {}) {
  const defaults = DEFAULT_TERRAIN_GENERATION_CONFIG;
  const gameplay = {};
  for (const [terrainType, rule] of Object.entries(defaults.gameplay))
    gameplay[terrainType] = {
      ...rule,
      ...(overrides.gameplay?.[terrainType] ?? {}),
    };
  return {
    maxElevation: Number.isFinite(overrides.maxElevation)
      ? Math.max(0, overrides.maxElevation)
      : defaults.maxElevation,
    thresholds: { ...defaults.thresholds, ...(overrides.thresholds ?? {}) },
    elevation: {
      ...defaults.elevation,
      ...(overrides.elevation ?? {}),
      broadNoise: {
        ...defaults.elevation.broadNoise,
        ...(overrides.elevation?.broadNoise ?? {}),
      },
      detailNoise: {
        ...defaults.elevation.detailNoise,
        ...(overrides.elevation?.detailNoise ?? {}),
      },
    },
    moisture: {
      ...defaults.moisture,
      ...(overrides.moisture ?? {}),
      broadNoise: {
        ...defaults.moisture.broadNoise,
        ...(overrides.moisture?.broadNoise ?? {}),
      },
      detailNoise: {
        ...defaults.moisture.detailNoise,
        ...(overrides.moisture?.detailNoise ?? {}),
      },
    },
    waterBodies: { ...defaults.waterBodies, ...(overrides.waterBodies ?? {}) },
    rivers: { ...defaults.rivers, ...(overrides.rivers ?? {}) },
    gameplay,
  };
}

function clamp(value, minimum = 0, maximum = 1) {
  return Math.max(minimum, Math.min(maximum, value));
}

function lerp(first, second, amount) {
  return first + (second - first) * amount;
}

function mixColor(first, second, amount) {
  return first.map((channel, index) =>
    Math.round(lerp(channel, second[index], clamp(amount))),
  );
}

export function terrainColor(sample, { debug = false } = {}) {
  const terrainType = sample.terrainType ?? sample.type ?? TERRAIN_TYPES.PLAINS;
  const palette = debug ? TERRAIN_DEBUG_PALETTE : TERRAIN_PALETTE;
  if (debug) return [...(palette[terrainType] ?? palette.plains)];
  let color = [...(palette[terrainType] ?? palette.plains)];
  if (terrainType === TERRAIN_TYPES.PLAINS)
    color = mixColor(color, [128, 142, 62], sample.moisture * 0.32);
  else if (terrainType === TERRAIN_TYPES.LOWLAND)
    color = mixColor(color, [75, 112, 52], sample.moisture * 0.38);
  else if (terrainType === TERRAIN_TYPES.HILL)
    color = mixColor(color, [116, 94, 58], (1 - sample.moisture) * 0.35);
  else if (terrainType === TERRAIN_TYPES.MOUNTAIN)
    color = mixColor(color, [132, 124, 104], sample.elevation * 0.28);
  const variation =
    terrainType === TERRAIN_TYPES.RIVER || terrainType.includes("water")
      ? (sample.moisture - 0.5) * 10
      : (sample.elevation - 0.5) * 17 + (sample.moisture - 0.5) * 8;
  return color.map((channel) => clamp(Math.round(channel + variation), 0, 255));
}

export function generateTerrain({ bounds, seed, config = {} }) {
  const resolvedConfig = mergeConfig(config);
  const spanX = Math.max(1, bounds.maxX - bounds.minX);
  const spanY = Math.max(1, bounds.maxY - bounds.minY);
  const width = Math.max(1, Math.ceil(spanX));
  const height = Math.max(1, Math.ceil(spanY));
  const numericSeed = seedFrom(seed);
  const elevation = generateElevationMap(
    width,
    height,
    deriveSeed(numericSeed, "elevation-map"),
    resolvedConfig.elevation,
  );
  const moisture = generateMoistureMap(
    width,
    height,
    deriveSeed(numericSeed, "moisture-map"),
    resolvedConfig.moisture,
  );
  const hydrology = generateRivers({
    elevation,
    moisture,
    width,
    height,
    seed: deriveSeed(numericSeed, "river-map"),
    waterLevel: resolvedConfig.thresholds.shallowWater,
    config: resolvedConfig.rivers,
  });
  const terrainTypes = new Array(width * height);
  for (let index = 0; index < terrainTypes.length; index += 1) {
    const base = classifyElevation(elevation[index], resolvedConfig.thresholds);
    terrainTypes[index] =
      hydrology.river[index] &&
      base !== TERRAIN_TYPES.DEEP_WATER &&
      base !== TERRAIN_TYPES.SHALLOW_WATER
        ? TERRAIN_TYPES.RIVER
        : base;
  }
  const waterSurfaces = generateWaterSurfaceMap({
    elevation,
    terrainTypes,
    width,
    height,
    shallowWaterLevel: resolvedConfig.thresholds.shallowWater,
    config: resolvedConfig.waterBodies,
  });

  const gridCoordinates = (mapX, mapY) => ({
    x: clamp(((mapX - bounds.minX) / spanX) * width - 0.5, 0, width - 1),
    y: clamp(((mapY - bounds.minY) / spanY) * height - 0.5, 0, height - 1),
  });
  const indexAtGrid = (x, y) =>
    Math.max(0, Math.min(height - 1, y)) * width +
    Math.max(0, Math.min(width - 1, x));
  const interpolatedValue = (values, gridX, gridY) => {
    const x0 = Math.floor(gridX);
    const y0 = Math.floor(gridY);
    const x1 = Math.min(width - 1, x0 + 1);
    const y1 = Math.min(height - 1, y0 + 1);
    const amountX = gridX - x0;
    const amountY = gridY - y0;
    return lerp(
      lerp(values[indexAtGrid(x0, y0)], values[indexAtGrid(x1, y0)], amountX),
      lerp(values[indexAtGrid(x0, y1)], values[indexAtGrid(x1, y1)], amountX),
      amountY,
    );
  };

  const cellAtGrid = (x, y) => {
    const cellX = Math.max(0, Math.min(width - 1, Math.round(x)));
    const cellY = Math.max(0, Math.min(height - 1, Math.round(y)));
    const index = indexAtGrid(cellX, cellY);
    const terrainType = terrainTypes[index];
    const gameplay = terrainGameplayProperties(
      terrainType,
      resolvedConfig.gameplay,
    );
    return {
      x: cellX,
      y: cellY,
      elevation: elevation[index],
      surfaceElevation: waterSurfaces.surfaceElevation[index],
      moisture: moisture[index],
      terrainType,
      type: terrainType,
      river: Boolean(hydrology.river[index]),
      riverFlow: hydrology.river[index] ? hydrology.flow[index] : undefined,
      movementCost: gameplay.movementCost,
      passable: gameplay.passable,
      water: gameplay.water,
      waterCoverage: Number(gameplay.water),
    };
  };

  const sample = (mapX, mapY) => {
    const grid = gridCoordinates(mapX, mapY);
    const nearest = cellAtGrid(grid.x, grid.y);
    const sampleElevation = interpolatedValue(elevation, grid.x, grid.y);
    const sampleSurfaceElevation = interpolatedValue(
      waterSurfaces.surfaceElevation,
      grid.x,
      grid.y,
    );
    const sampleMoisture = interpolatedValue(moisture, grid.x, grid.y);
    const riverCoverage = interpolatedValue(hydrology.river, grid.x, grid.y);
    const sampleRiverFlow = interpolatedValue(hydrology.flow, grid.x, grid.y);
    const baseType = classifyElevation(
      sampleElevation,
      resolvedConfig.thresholds,
    );
    const terrainType =
      riverCoverage >= 0.36 &&
      baseType !== TERRAIN_TYPES.DEEP_WATER &&
      baseType !== TERRAIN_TYPES.SHALLOW_WATER
        ? TERRAIN_TYPES.RIVER
        : baseType;
    const gameplay = terrainGameplayProperties(
      terrainType,
      resolvedConfig.gameplay,
    );
    return {
      ...nearest,
      elevation: sampleElevation,
      surfaceElevation: sampleSurfaceElevation,
      moisture: sampleMoisture,
      terrainType,
      type: terrainType,
      river: riverCoverage >= 0.5,
      riverCoverage,
      riverFlow: riverCoverage > 0 ? sampleRiverFlow : undefined,
      movementCost: gameplay.movementCost,
      passable: gameplay.passable,
      water: gameplay.water,
      waterCoverage: Number(gameplay.water),
    };
  };

  return {
    seed: numericSeed,
    bounds: { ...bounds },
    width,
    height,
    config: resolvedConfig,
    elevation,
    surfaceElevation: waterSurfaces.surfaceElevation,
    flatWaterBodies: waterSurfaces.flatBodies,
    moisture,
    river: hydrology.river,
    riverFlow: hydrology.flow,
    downstream: hydrology.downstream,
    terrainTypes,
    sample,
    cellAtGrid,
    cellAtMapPoint(mapX, mapY) {
      const grid = gridCoordinates(mapX, mapY);
      return cellAtGrid(grid.x, grid.y);
    },
  };
}

export function createTerrainSampler(bounds, seed, config = {}) {
  const terrain = generateTerrain({ bounds, seed, config });
  const sampler = (x, y) => terrain.sample(x, y);
  sampler.terrain = terrain;
  return sampler;
}
