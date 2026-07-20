import {
  TERRAIN_TYPES,
  isWaterTerrain,
  normalizeTerrainType,
} from "./terrain-types.js";

export const DEFAULT_TERRAIN_GAMEPLAY_RULES = Object.freeze({
  deep_water: Object.freeze({
    movementCost: Number.POSITIVE_INFINITY,
    passable: false,
  }),
  shallow_water: Object.freeze({ movementCost: 5, passable: false }),
  river: Object.freeze({ movementCost: 2.5, passable: false }),
  lowland: Object.freeze({ movementCost: 1.2, passable: true }),
  plains: Object.freeze({ movementCost: 1, passable: true }),
  hill: Object.freeze({ movementCost: 1.8, passable: true }),
  mountain: Object.freeze({ movementCost: 4, passable: true }),
});

export function terrainGameplayProperties(
  terrainType,
  rules = DEFAULT_TERRAIN_GAMEPLAY_RULES,
) {
  const normalized = normalizeTerrainType(terrainType);
  const rule = rules[normalized] ?? rules[TERRAIN_TYPES.PLAINS];
  return {
    terrainType: normalized,
    movementCost: rule.movementCost,
    passable: rule.passable,
    water: isWaterTerrain(normalized) || normalized === TERRAIN_TYPES.RIVER,
  };
}
