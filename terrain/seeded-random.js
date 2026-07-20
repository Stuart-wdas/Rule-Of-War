const DEFAULT_SEED = 0x6d2b79f5;

/** Converts strings and numbers into the same unsigned seed on every client. */
export function seedFrom(value) {
  const text = String(value ?? "rule-of-war-terrain");
  let hash = DEFAULT_SEED;
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

export function deriveSeed(seed, channel) {
  return seedFrom(`${seedFrom(seed)}:${channel}`);
}

/**
 * Mulberry32 keeps generation deterministic without touching Math.random.
 * Separate derived seeds are used for elevation, moisture, and rivers so a
 * change to one system does not silently consume another system's sequence.
 */
export function createSeededRandom(seedValue) {
  let value = seedFrom(seedValue);
  return () => {
    value += 0x6d2b79f5;
    let mixed = value;
    mixed = Math.imul(mixed ^ (mixed >>> 15), mixed | 1);
    mixed ^= mixed + Math.imul(mixed ^ (mixed >>> 7), mixed | 61);
    return ((mixed ^ (mixed >>> 14)) >>> 0) / 4294967296;
  };
}

/** Stable random access to a lattice point; no mutable PRNG state is used. */
export function randomAt2D(x, y, seedValue) {
  const seed = seedFrom(seedValue);
  let value = Math.imul(x | 0, 374761393) ^ Math.imul(y | 0, 668265263);
  value = (value + Math.imul(seed, 1442695041)) | 0;
  value = Math.imul(value ^ (value >>> 13), 1274126177);
  return ((value ^ (value >>> 16)) >>> 0) / 4294967295;
}
