import { randomAt2D } from "./seeded-random.js";

function lerp(first, second, amount) {
  return first + (second - first) * amount;
}

function fade(value) {
  return value * value * (3 - 2 * value);
}

/** Smooth, deterministic lattice noise in the range 0..1. */
export function valueNoise2D(x, y, seed) {
  const cellX = Math.floor(x);
  const cellY = Math.floor(y);
  const amountX = fade(x - cellX);
  const amountY = fade(y - cellY);
  const top = lerp(
    randomAt2D(cellX, cellY, seed),
    randomAt2D(cellX + 1, cellY, seed),
    amountX,
  );
  const bottom = lerp(
    randomAt2D(cellX, cellY + 1, seed),
    randomAt2D(cellX + 1, cellY + 1, seed),
    amountX,
  );
  return lerp(top, bottom, amountY);
}

/** Layers progressively smaller noise to add detail without losing continents. */
export function fractalNoise2D(
  x,
  y,
  seed,
  { octaves = 4, persistence = 0.5, lacunarity = 2 } = {},
) {
  let amplitude = 1;
  let frequency = 1;
  let total = 0;
  let normalization = 0;
  for (let octave = 0; octave < octaves; octave += 1) {
    total +=
      valueNoise2D(x * frequency, y * frequency, seed + octave * 1013) *
      amplitude;
    normalization += amplitude;
    amplitude *= persistence;
    frequency *= lacunarity;
  }
  return normalization > 0 ? total / normalization : 0;
}
