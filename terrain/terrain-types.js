export const TERRAIN_TYPES = Object.freeze({
  DEEP_WATER: "deep_water",
  SHALLOW_WATER: "shallow_water",
  RIVER: "river",
  LOWLAND: "lowland",
  PLAINS: "plains",
  HILL: "hill",
  MOUNTAIN: "mountain",
});

export const TERRAIN_TYPE_VALUES = Object.freeze(Object.values(TERRAIN_TYPES));

export const DEFAULT_TERRAIN_THRESHOLDS = Object.freeze({
  deepWater: 0.2,
  shallowWater: 0.3,
  lowland: 0.42,
  plains: 0.65,
  hill: 0.82,
});

export const TERRAIN_PALETTE = Object.freeze({
  deep_water: Object.freeze([24, 62, 96]),
  shallow_water: Object.freeze([43, 103, 139]),
  river: Object.freeze([36, 112, 161]),
  lowland: Object.freeze([91, 99, 48]),
  plains: Object.freeze([91, 126, 55]),
  hill: Object.freeze([91, 101, 54]),
  mountain: Object.freeze([104, 99, 83]),
});

export const TERRAIN_DEBUG_PALETTE = Object.freeze({
  deep_water: Object.freeze([16, 52, 120]),
  shallow_water: Object.freeze([39, 142, 214]),
  river: Object.freeze([0, 218, 255]),
  lowland: Object.freeze([132, 101, 43]),
  plains: Object.freeze([100, 184, 74]),
  hill: Object.freeze([214, 165, 55]),
  mountain: Object.freeze([206, 210, 219]),
});

export function classifyElevation(
  elevation,
  thresholds = DEFAULT_TERRAIN_THRESHOLDS,
) {
  if (elevation < thresholds.deepWater) return TERRAIN_TYPES.DEEP_WATER;
  if (elevation < thresholds.shallowWater)
    return TERRAIN_TYPES.SHALLOW_WATER;
  if (elevation < thresholds.lowland) return TERRAIN_TYPES.LOWLAND;
  if (elevation < thresholds.plains) return TERRAIN_TYPES.PLAINS;
  if (elevation < thresholds.hill) return TERRAIN_TYPES.HILL;
  return TERRAIN_TYPES.MOUNTAIN;
}

export function isWaterTerrain(terrainType) {
  return (
    terrainType === TERRAIN_TYPES.DEEP_WATER ||
    terrainType === TERRAIN_TYPES.SHALLOW_WATER
  );
}

export function normalizeTerrainType(value) {
  if (TERRAIN_TYPE_VALUES.includes(value)) return value;
  if (value === "water") return TERRAIN_TYPES.SHALLOW_WATER;
  if (value === "grass" || value === "meadow") return TERRAIN_TYPES.PLAINS;
  if (value === "mud" || value === "scrub") return TERRAIN_TYPES.LOWLAND;
  return TERRAIN_TYPES.PLAINS;
}
