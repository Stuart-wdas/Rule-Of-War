import {
  generateElevationField,
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

// Elevation is expressed relative to one map cell. Values much larger than a
// cell create cliff walls and make foundations appear buried or suspended.
export const TERRAIN_MAX_ELEVATION = 3;

export const DEFAULT_TERRAIN_GENERATION_CONFIG = Object.freeze({
  maxElevation: TERRAIN_MAX_ELEVATION,
  thresholds: DEFAULT_TERRAIN_THRESHOLDS,
  // Wavelengths are measured in map tiles. Increase them for broader, slower
  // changes; reduce them for tighter terrain detail. Octaves/persistence/
  // lacunarity below control how many noise layers are combined.
  elevation: Object.freeze({
    broadWavelength: 90,
    ridgeWavelength: 38,
    detailWavelength: 10,
    warpWavelength: 72,
    warpStrength: 11,
    broadWeight: 0.62,
    ridgeWeight: 0.28,
    detailWeight: 0.1,
    mountainMaskStart: 0.48,
    mountainMaskEnd: 0.76,
    elevationExponent: 1.2,
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
    ridgeNoise: Object.freeze({
      octaves: 5,
      persistence: 0.53,
      lacunarity: 2.02,
    }),
  }),
  moisture: Object.freeze({
    broadWavelength: 23,
    detailWavelength: 5,
    warpWavelength: 31,
    warpStrength: 4.5,
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
    minimumFlatCells: 1,
    includeDiagonalConnections: true,
    shorelineOffset: 0.006,
    maximumSurfaceLift: 0.12,
    fixedSurfaceElevation: null,
    shorelineBlendWidth: 1.1,
    maximumOpenWaterCoverage: 0.28,
    maximumTotalWaterCoverage: 0.3,
    coverageControlMinimumCells: 1,
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
    widthCells: 1,
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
      ridgeNoise: {
        ...defaults.elevation.ridgeNoise,
        ...(overrides.elevation?.ridgeNoise ?? {}),
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

function elevationThresholdAtCoverage(values, coverage, bucketCount = 2048) {
  if (coverage <= 0 || values.length === 0) return 0;
  const buckets = new Uint32Array(bucketCount);
  for (const value of values)
    buckets[Math.min(bucketCount - 1, Math.floor(clamp(value) * bucketCount))] +=
      1;
  const target = Math.max(1, Math.floor(values.length * clamp(coverage)));
  let accumulated = 0;
  for (let index = 0; index < buckets.length; index += 1) {
    accumulated += buckets[index];
    if (accumulated >= target) return index / bucketCount;
  }
  return 1;
}

function mixColor(first, second, amount) {
  return first.map((channel, index) =>
    Math.round(lerp(channel, second[index], clamp(amount))),
  );
}

export function terrainColor(
  sample,
  { debug = false, x = sample.x ?? 0, y = sample.y ?? 0 } = {},
) {
  const terrainType = sample.terrainType ?? sample.type ?? TERRAIN_TYPES.PLAINS;
  const palette = debug ? TERRAIN_DEBUG_PALETTE : TERRAIN_PALETTE;
  if (debug) return [...(palette[terrainType] ?? palette.plains)];
  let color = Array.isArray(sample.authoredColor)
    ? [...sample.authoredColor]
    : [...(palette[terrainType] ?? palette.plains)];
  if (terrainType === TERRAIN_TYPES.PLAINS)
    color = mixColor(color, [128, 142, 62], sample.moisture * 0.32);
  else if (terrainType === TERRAIN_TYPES.LOWLAND)
    color = mixColor(color, [75, 112, 52], sample.moisture * 0.38);
  else if (terrainType === TERRAIN_TYPES.HILL)
    color = mixColor(color, [116, 94, 58], (1 - sample.moisture) * 0.35);
  else if (terrainType === TERRAIN_TYPES.MOUNTAIN)
    color = mixColor(color, [132, 124, 104], sample.elevation * 0.28);
  const water =
    terrainType === TERRAIN_TYPES.RIVER || terrainType.includes("water");
  const broadVariation =
    Math.sin(x * 0.17 + y * 0.11) * 0.5 +
    Math.sin(x * 0.047 - y * 0.083 + 1.7) * 0.5;
  const fineNoise = Math.sin(x * 12.9898 + y * 78.233) * 43758.5453;
  const fineVariation = fineNoise - Math.floor(fineNoise) - 0.5;
  const variation = water
    ? (sample.moisture - 0.5) * 10 + broadVariation * 2
    : (sample.elevation - 0.5) * 22 +
      (sample.moisture - 0.5) * 9 +
      broadVariation * 11 +
      fineVariation * 5;
  const elevationWarmth = water ? 0 : (sample.elevation - 0.5) * 7;
  return color.map((channel, index) =>
    clamp(
      Math.round(
        channel +
          variation +
          (index === 0 ? elevationWarmth : index === 2 ? -elevationWarmth : 0),
      ),
      0,
      255,
    ),
  );
}

export function generateTerrain({ bounds, seed, config = {} }) {
  const resolvedConfig = mergeConfig(config);
  const spanX = Math.max(1, bounds.maxX - bounds.minX);
  const spanY = Math.max(1, bounds.maxY - bounds.minY);
  const resolutionScale = Number.isFinite(config.resolutionScale)
    ? clamp(config.resolutionScale, 0.02, 1)
    : 1;
  const width = Math.max(1, Math.ceil(spanX * resolutionScale));
  const height = Math.max(1, Math.ceil(spanY * resolutionScale));
  const numericSeed = seedFrom(seed);
  const elevationField = generateElevationField(
    width,
    height,
    deriveSeed(numericSeed, "elevation-map"),
    resolvedConfig.elevation,
  );
  const elevation = elevationField.values;
  const configuredThresholds = resolvedConfig.thresholds;
  const controlWaterCoverage =
    width * height >= resolvedConfig.waterBodies.coverageControlMinimumCells;
  const coverageSeaLevel = controlWaterCoverage
    ? elevationThresholdAtCoverage(
        elevation,
        resolvedConfig.waterBodies.maximumOpenWaterCoverage,
      )
    : configuredThresholds.shallowWater;
  const shallowWaterLevel = Math.min(
    configuredThresholds.shallowWater,
    coverageSeaLevel,
  );
  const deepWaterRatio =
    configuredThresholds.deepWater /
    Math.max(configuredThresholds.shallowWater, 1e-6);
  let terrainThresholds = {
    ...configuredThresholds,
    deepWater: Math.min(
      configuredThresholds.deepWater,
      shallowWaterLevel * deepWaterRatio,
    ),
    shallowWater: shallowWaterLevel,
  };
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
    waterLevel: terrainThresholds.shallowWater,
    config: resolvedConfig.rivers,
  });
  if (resolvedConfig.rivers.widthCells > 0) {
    const originalRiver = new Uint8Array(hydrology.river);
    const originalFlow = new Float32Array(hydrology.flow);
    const radius = Math.max(0, Math.floor(resolvedConfig.rivers.widthCells));
    for (let index = 0; index < originalRiver.length; index += 1) {
      if (!originalRiver[index]) continue;
      const centerX = index % width;
      const centerY = Math.floor(index / width);
      for (let offsetY = -radius; offsetY <= radius; offsetY += 1)
        for (let offsetX = -radius; offsetX <= radius; offsetX += 1) {
          if (Math.abs(offsetX) + Math.abs(offsetY) > radius) continue;
          const x = centerX + offsetX;
          const y = centerY + offsetY;
          if (x < 0 || y < 0 || x >= width || y >= height) continue;
          const widenedIndex = y * width + x;
          hydrology.river[widenedIndex] = 1;
          hydrology.flow[widenedIndex] = Math.max(
            hydrology.flow[widenedIndex],
            originalFlow[index] * 0.7,
          );
        }
    }
  }

  // Rivers count toward the same hard water budget as lakes and seas. Reserve
  // room for every river cell, then lower sea level enough that the combined
  // classified coverage cannot exceed the configured percentage at any scale.
  const maximumWaterCells = Math.floor(
    width *
      height *
      clamp(resolvedConfig.waterBodies.maximumTotalWaterCoverage, 0, 1),
  );
  let riverCells = 0;
  for (const river of hydrology.river) riverCells += Number(Boolean(river));
  if (riverCells > maximumWaterCells) {
    const riverIndices = [];
    for (let index = 0; index < hydrology.river.length; index += 1)
      if (hydrology.river[index]) riverIndices.push(index);
    riverIndices.sort(
      (first, second) => hydrology.flow[first] - hydrology.flow[second],
    );
    for (let index = 0; index < riverCells - maximumWaterCells; index += 1) {
      hydrology.river[riverIndices[index]] = 0;
      hydrology.flow[riverIndices[index]] = 0;
    }
    riverCells = maximumWaterCells;
  }
  const openWaterBudget = Math.max(0, maximumWaterCells - riverCells);
  const cappedShallowWaterLevel = elevationThresholdAtCoverage(
    elevation,
    openWaterBudget / Math.max(1, width * height),
  );
  const finalShallowWaterLevel = Math.min(
    terrainThresholds.shallowWater,
    cappedShallowWaterLevel,
  );
  terrainThresholds = {
    ...terrainThresholds,
    deepWater: Math.min(
      terrainThresholds.deepWater,
      finalShallowWaterLevel * deepWaterRatio,
    ),
    shallowWater: finalShallowWaterLevel,
  };
  const terrainTypes = new Array(width * height);
  for (let index = 0; index < terrainTypes.length; index += 1) {
    const base = classifyElevation(elevation[index], terrainThresholds);
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
    shallowWaterLevel: terrainThresholds.shallowWater,
    config: resolvedConfig.waterBodies,
  });
  const openWater = terrainTypes.map(
    (terrainType) =>
      terrainType === TERRAIN_TYPES.DEEP_WATER ||
      terrainType === TERRAIN_TYPES.SHALLOW_WATER ||
      terrainType === TERRAIN_TYPES.RIVER,
  );
  const shorelineBlend = new Float32Array(width * height);
  const shorelineWaterLevel = new Float32Array(width * height);
  const blendWidth = Math.max(
    0.05,
    resolvedConfig.waterBodies.shorelineBlendWidth,
  );
  const searchRadius = Math.max(2, Math.ceil(blendWidth) + 1);

  // Rendering uses a signed distance-like field around the cell water mask.
  // Gameplay still sees exact water cells, while visual vertices transition
  // gradually between continuous land noise and a connected body's flat level.
  for (let index = 0; index < openWater.length; index += 1) {
    const centerX = index % width;
    const centerY = Math.floor(index / width);
    const waterCell = openWater[index];
    let oppositeDistance = Number.POSITIVE_INFINITY;
    let nearestWaterDistance = waterCell ? 0 : Number.POSITIVE_INFINITY;
    let nearestWaterLevel = waterCell
      ? waterSurfaces.surfaceElevation[index]
      : elevation[index];
    for (let offsetY = -searchRadius; offsetY <= searchRadius; offsetY += 1)
      for (let offsetX = -searchRadius; offsetX <= searchRadius; offsetX += 1) {
        const x = centerX + offsetX;
        const y = centerY + offsetY;
        if (x < 0 || y < 0 || x >= width || y >= height) continue;
        const candidate = y * width + x;
        const candidateDistance = Math.hypot(offsetX, offsetY);
        if (openWater[candidate] !== waterCell)
          oppositeDistance = Math.min(oppositeDistance, candidateDistance);
        if (openWater[candidate] && candidateDistance < nearestWaterDistance) {
          nearestWaterDistance = candidateDistance;
          nearestWaterLevel = waterSurfaces.surfaceElevation[candidate];
        }
      }
    const signedDistance = Number.isFinite(oppositeDistance)
      ? waterCell
        ? oppositeDistance
        : -oppositeDistance
      : waterCell
        ? blendWidth
        : -blendWidth;
    const amount = Math.max(
      0,
      Math.min(1, signedDistance / (blendWidth * 2) + 0.5),
    );
    shorelineBlend[index] = amount * amount * (3 - 2 * amount);
    shorelineWaterLevel[index] = nearestWaterLevel;
  }

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
    const continuousLandElevation = elevationField.sample(grid.x, grid.y);
    const waterLevelAtPoint = interpolatedValue(
      shorelineWaterLevel,
      grid.x,
      grid.y,
    );
    const shorelineAmount = interpolatedValue(
      shorelineBlend,
      grid.x,
      grid.y,
    );
    const sampleMoisture = interpolatedValue(moisture, grid.x, grid.y);
    const riverCoverage = interpolatedValue(hydrology.river, grid.x, grid.y);
    const sampleRiverFlow = interpolatedValue(hydrology.flow, grid.x, grid.y);
    const baseType = classifyElevation(
      sampleElevation,
      terrainThresholds,
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
    // Bilinear terrain interpolation must not reintroduce waves into a lake
    // or sea. Every classified open-water sample uses its connected body's
    // already-levelled surface height.
    const sampleSurfaceElevation = lerp(
      continuousLandElevation,
      waterLevelAtPoint,
      shorelineAmount,
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
    config: { ...resolvedConfig, thresholds: terrainThresholds },
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

const SERIAL_TERRAIN_TYPES = Object.freeze(Object.values(TERRAIN_TYPES));

export function serializeTerrain(terrain) {
  const terrainTypeCodes = new Uint8Array(terrain.terrainTypes.length);
  for (let index = 0; index < terrain.terrainTypes.length; index += 1)
    terrainTypeCodes[index] = Math.max(
      0,
      SERIAL_TERRAIN_TYPES.indexOf(terrain.terrainTypes[index]),
    );
  return {
    version: 1,
    seed: terrain.seed,
    bounds: terrain.bounds,
    width: terrain.width,
    height: terrain.height,
    config: terrain.config,
    elevation: terrain.elevation,
    surfaceElevation: terrain.surfaceElevation,
    moisture: terrain.moisture,
    river: terrain.river,
    riverFlow: terrain.riverFlow,
    downstream: terrain.downstream,
    terrainTypeCodes,
  };
}

export function hydrateTerrain(data) {
  if (!data || data.version !== 1) return null;
  const { bounds, width, height } = data;
  const hexColor = (value, fallback) => {
    const match = /^#?([0-9a-f]{6})$/i.exec(String(value ?? ""));
    if (!match) return fallback;
    const number = Number.parseInt(match[1], 16);
    return [(number >> 16) & 255, (number >> 8) & 255, number & 255];
  };
  const lowColor = hexColor(data.config.heightColors?.low, [72, 111, 48]);
  const middleColor = hexColor(data.config.heightColors?.mid, [126, 139, 69]);
  const highColor = hexColor(data.config.heightColors?.high, [181, 171, 126]);
  const authoredColor = (elevation) => {
    if (!data.config.heightColors) return undefined;
    const lowBoundary = data.config.heightColors.lowBoundary ?? 0.45;
    const highBoundary = data.config.heightColors.highBoundary ?? 0.72;
    return elevation < lowBoundary
      ? lowColor
      : elevation < highBoundary
        ? middleColor
        : highColor;
  };
  const indexAt = (x, y) => y * width + x;
  const gridCoordinates = (mapX, mapY) => ({
    x: clamp(
      ((mapX - bounds.minX) / Math.max(bounds.maxX - bounds.minX, 1)) *
        (width - 1),
      0,
      width - 1,
    ),
    y: clamp(
      ((mapY - bounds.minY) / Math.max(bounds.maxY - bounds.minY, 1)) *
        (height - 1),
      0,
      height - 1,
    ),
  });
  const interpolate = (values, x, y) => {
    const x0 = Math.floor(x);
    const y0 = Math.floor(y);
    const x1 = Math.min(width - 1, x0 + 1);
    const y1 = Math.min(height - 1, y0 + 1);
    const amountX = x - x0;
    const amountY = y - y0;
    return lerp(
      lerp(values[indexAt(x0, y0)], values[indexAt(x1, y0)], amountX),
      lerp(values[indexAt(x0, y1)], values[indexAt(x1, y1)], amountX),
      amountY,
    );
  };
  const cellAtGrid = (x, y) => {
    const cellX = clamp(Math.round(x), 0, width - 1);
    const cellY = clamp(Math.round(y), 0, height - 1);
    const index = indexAt(cellX, cellY);
    const terrainType =
      SERIAL_TERRAIN_TYPES[data.terrainTypeCodes[index]] ?? TERRAIN_TYPES.PLAINS;
    const gameplay = terrainGameplayProperties(terrainType, data.config.gameplay);
    return {
      x: cellX,
      y: cellY,
      elevation: data.elevation[index],
      surfaceElevation: data.surfaceElevation[index],
      moisture: data.moisture[index],
      terrainType,
      type: terrainType,
      river: Boolean(data.river[index]),
      riverFlow: data.river[index] ? data.riverFlow[index] : undefined,
      movementCost: gameplay.movementCost,
      passable: gameplay.passable,
      water: gameplay.water,
      waterCoverage: Number(gameplay.water),
      authoredColor: gameplay.water
        ? undefined
        : authoredColor(data.elevation[index]),
    };
  };
  const sample = (mapX, mapY) => {
    const grid = gridCoordinates(mapX, mapY);
    const nearest = cellAtGrid(grid.x, grid.y);
    const riverCoverage = interpolate(data.river, grid.x, grid.y);
    return {
      ...nearest,
      elevation: interpolate(data.elevation, grid.x, grid.y),
      surfaceElevation: interpolate(data.surfaceElevation, grid.x, grid.y),
      moisture: interpolate(data.moisture, grid.x, grid.y),
      authoredColor: nearest.water
        ? undefined
        : authoredColor(interpolate(data.elevation, grid.x, grid.y)),
      river: riverCoverage >= 0.5,
      riverCoverage,
      riverFlow:
        riverCoverage > 0
          ? interpolate(data.riverFlow, grid.x, grid.y)
          : undefined,
    };
  };
  return {
    ...data,
    terrainTypes: data.terrainTypeCodes,
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
