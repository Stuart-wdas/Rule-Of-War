import { fractalNoise2D, valueNoise2D } from "./noise.js";
import { deriveSeed } from "./seeded-random.js";

function normalizeMap(values) {
  let minimum = Number.POSITIVE_INFINITY;
  let maximum = Number.NEGATIVE_INFINITY;
  for (const value of values) {
    minimum = Math.min(minimum, value);
    maximum = Math.max(maximum, value);
  }
  const span = maximum - minimum;
  if (span <= 1e-9) {
    values.fill(0.5);
    return values;
  }
  for (let index = 0; index < values.length; index += 1)
    values[index] = Math.max(0, Math.min(1, (values[index] - minimum) / span));
  return values;
}

function generateLayeredMap(width, height, seed, config) {
  const values = new Float32Array(width * height);
  const broadSeed = deriveSeed(seed, "broad");
  const detailSeed = deriveSeed(seed, "detail");
  const warpSeed = deriveSeed(seed, "warp");
  const broadWavelength = Math.max(1, config.broadWavelength);
  const detailWavelength = Math.max(1, config.detailWavelength);
  const warpWavelength = Math.max(1, config.warpWavelength);
  const weightTotal = Math.max(config.broadWeight + config.detailWeight, 1e-6);

  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const warp =
        (valueNoise2D(x / warpWavelength, y / warpWavelength, warpSeed) -
          0.5) *
        config.warpStrength;
      const broad = fractalNoise2D(
        (x + warp) / broadWavelength,
        (y - warp * 0.73) / broadWavelength,
        broadSeed,
        config.broadNoise,
      );
      const detail = fractalNoise2D(
        (x - warp * 0.31) / detailWavelength,
        (y + warp * 0.47) / detailWavelength,
        detailSeed,
        config.detailNoise,
      );
      values[y * width + x] =
        (broad * config.broadWeight + detail * config.detailWeight) /
        weightTotal;
    }
  }
  return normalizeMap(values);
}

export function generateElevationMap(width, height, seed, config) {
  return generateLayeredMap(width, height, deriveSeed(seed, "elevation"), config);
}

export function generateMoistureMap(width, height, seed, config) {
  return generateLayeredMap(width, height, deriveSeed(seed, "moisture"), config);
}
