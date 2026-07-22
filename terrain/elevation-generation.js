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

function layeredNoiseValue(x, y, seed, config) {
  const broadSeed = deriveSeed(seed, "broad");
  const detailSeed = deriveSeed(seed, "detail");
  const warpSeed = deriveSeed(seed, "warp");
  const ridgeSeed = deriveSeed(seed, "ridges");
  const broadWavelength = Math.max(1, config.broadWavelength);
  const detailWavelength = Math.max(1, config.detailWavelength);
  const warpWavelength = Math.max(1, config.warpWavelength);
  const ridgeWeight = Math.max(0, config.ridgeWeight ?? 0);
  const weightTotal = Math.max(
    config.broadWeight + config.detailWeight + ridgeWeight,
    1e-6,
  );
  const warp =
    (valueNoise2D(x / warpWavelength, y / warpWavelength, warpSeed) - 0.5) *
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
  let mountainContribution = 0;
  if (ridgeWeight > 0) {
    const ridgeWavelength = Math.max(1, config.ridgeWavelength ?? 38);
    const ridgeNoise = fractalNoise2D(
      (x + warp * 0.58) / ridgeWavelength,
      (y - warp * 0.41) / ridgeWavelength,
      ridgeSeed,
      config.ridgeNoise,
    );
    const ridge = (1 - Math.abs(ridgeNoise * 2 - 1)) ** 2;
    const maskStart = config.mountainMaskStart ?? 0.48;
    const maskEnd = Math.max(maskStart + 1e-6, config.mountainMaskEnd ?? 0.76);
    const maskAmount = Math.max(
      0,
      Math.min(1, (broad - maskStart) / (maskEnd - maskStart)),
    );
    const mountainMask = maskAmount * maskAmount * (3 - 2 * maskAmount);
    mountainContribution = ridge * mountainMask * ridgeWeight;
  }
  return (
    (broad * config.broadWeight +
      detail * config.detailWeight +
      mountainContribution) /
    weightTotal
  );
}

function generateLayeredField(width, height, seed, config) {
  const values = new Float32Array(width * height);
  let minimum = Number.POSITIVE_INFINITY;
  let maximum = Number.NEGATIVE_INFINITY;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const value = layeredNoiseValue(x, y, seed, config);
      values[y * width + x] = value;
      minimum = Math.min(minimum, value);
      maximum = Math.max(maximum, value);
    }
  }
  normalizeMap(values);
  const elevationExponent = Math.max(0.01, config.elevationExponent ?? 1);
  if (Math.abs(elevationExponent - 1) > 1e-6)
    for (let index = 0; index < values.length; index += 1)
      values[index] = values[index] ** elevationExponent;
  const span = Math.max(maximum - minimum, 1e-9);
  return {
    values,
    sample(x, y) {
      const normalized = Math.max(
        0,
        Math.min(1, (layeredNoiseValue(x, y, seed, config) - minimum) / span),
      );
      return normalized ** elevationExponent;
    },
  };
}

export function generateElevationMap(width, height, seed, config) {
  return generateElevationField(width, height, seed, config).values;
}

/** Raster values for gameplay plus a continuous sampler for visual height. */
export function generateElevationField(width, height, seed, config) {
  return generateLayeredField(
    width,
    height,
    deriveSeed(seed, "elevation"),
    config,
  );
}

export function generateMoistureMap(width, height, seed, config) {
  return generateLayeredField(
    width,
    height,
    deriveSeed(seed, "moisture"),
    config,
  ).values;
}
