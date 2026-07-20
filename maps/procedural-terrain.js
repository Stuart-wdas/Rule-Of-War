// Compatibility facade for the existing map/rendering imports. Terrain now
// comes from the modular cell generator under /terrain.
export {
  DEFAULT_TERRAIN_GENERATION_CONFIG,
  TERRAIN_MAX_ELEVATION,
  createTerrainSampler,
  generateTerrain,
  terrainColor,
} from "../terrain/terrain-generator.js";

export {
  DEFAULT_TERRAIN_THRESHOLDS,
  TERRAIN_DEBUG_PALETTE,
  TERRAIN_PALETTE,
  TERRAIN_TYPES,
  TERRAIN_TYPE_VALUES,
  classifyElevation,
  isWaterTerrain,
  normalizeTerrainType,
} from "../terrain/terrain-types.js";

export {
  DEFAULT_TERRAIN_GAMEPLAY_RULES,
  terrainGameplayProperties,
} from "../terrain/terrain-gameplay-rules.js";

/**
 * Keeps the old tile-classification call site while returning the complete
 * cell contract required by gameplay. The generator is sampled at the tile's
 * canonical terrain point; rendering can still sample continuously.
 */
export function classifyTerrainTile(sampleTerrain, center) {
  const sampled = sampleTerrain(center.x, center.y);
  return {
    x: sampled.x,
    y: sampled.y,
    elevation: sampled.elevation,
    surfaceElevation: sampled.surfaceElevation,
    moisture: sampled.moisture,
    terrainType: sampled.terrainType,
    type: sampled.terrainType,
    river: sampled.river,
    riverFlow: sampled.riverFlow,
    movementCost: sampled.movementCost,
    passable: sampled.passable,
    water: sampled.water,
    waterCoverage: sampled.waterCoverage,
  };
}
