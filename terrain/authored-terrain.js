import { MAP_BUILDER_HEIGHT_SCALE } from "../config/map-builder-config.js";
import { DEFAULT_TERRAIN_GENERATION_CONFIG } from "./terrain-generator.js";
import { TERRAIN_TYPES } from "./terrain-types.js";

const typeCodes = Object.values(TERRAIN_TYPES);

export function createAuthoredTerrainData(map) {
  const width = map.cols;
  const height = map.rows;
  const count = width * height;
  const source = map.authoredTerrain ?? {};
  const elevation = Float32Array.from(source.elevation ?? [], (value) =>
    Math.max(0, Math.min(1, Number(value) || 0)),
  );
  const fittedElevation =
    elevation.length === count ? elevation : new Float32Array(count).fill(0.5);
  const moisture = new Float32Array(count).fill(0.62);
  const river = new Uint8Array(count);
  const riverFlow = new Float32Array(count);
  const downstream = new Int32Array(count).fill(-1);
  const terrainTypeCodes = new Uint8Array(count);
  const surfaceElevation = new Float32Array(fittedElevation);
  const waterLevel = Math.max(
    0.02,
    Math.min(0.75, Number(source.waterLevel) || 0.28),
  );
  for (let index = 0; index < count; index += 1) {
    const value = fittedElevation[index];
    const type =
      value < waterLevel * 0.68
        ? TERRAIN_TYPES.DEEP_WATER
        : value < waterLevel
          ? TERRAIN_TYPES.SHALLOW_WATER
          : value < 0.42
            ? TERRAIN_TYPES.LOWLAND
            : value < 0.65
              ? TERRAIN_TYPES.PLAINS
              : value < 0.82
                ? TERRAIN_TYPES.HILL
                : TERRAIN_TYPES.MOUNTAIN;
    terrainTypeCodes[index] = typeCodes.indexOf(type);
    if (value < waterLevel) surfaceElevation[index] = waterLevel;
  }
  return {
    version: 1,
    seed: map.id,
    bounds: { minX: -0.5, maxX: width - 0.5, minY: -0.5, maxY: height - 0.5 },
    width,
    height,
    config: {
      ...DEFAULT_TERRAIN_GENERATION_CONFIG,
      gameplay: DEFAULT_TERRAIN_GENERATION_CONFIG.gameplay,
      maxElevation: MAP_BUILDER_HEIGHT_SCALE,
      heightColors: source.heightColors,
    },
    elevation: fittedElevation,
    surfaceElevation,
    moisture,
    river,
    riverFlow,
    downstream,
    terrainTypeCodes,
  };
}
