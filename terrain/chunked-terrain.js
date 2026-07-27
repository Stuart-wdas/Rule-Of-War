const coordinateKey = (x, y) => `${x},${y}`;

/**
 * Sparse, LRU-cached terrain overlay.
 *
 * Procedural terrain remains the immutable base. Roads, craters, destroyed
 * bridges, trenches, and other campaign changes are stored only in the chunk
 * where they occur, keeping persistent world saves proportional to changes.
 */
export function createChunkedTerrainSampler(
  baseSampler,
  {
    chunkSize = 64,
    maximumResidentChunks = 48,
    modifications = [],
  } = {},
) {
  if (typeof baseSampler !== "function")
    throw new TypeError("A base terrain sampler is required.");
  const chunks = new Map();
  let accessSequence = 0;

  const chunkCoordinates = (x, y) => ({
    chunkX: Math.floor(x / chunkSize),
    chunkY: Math.floor(y / chunkSize),
  });
  const chunkKey = (chunkX, chunkY) => `${chunkX},${chunkY}`;
  const getChunk = (x, y, create = true) => {
    const { chunkX, chunkY } = chunkCoordinates(x, y);
    const key = chunkKey(chunkX, chunkY);
    let chunk = chunks.get(key);
    if (!chunk && create) {
      chunk = {
        key,
        chunkX,
        chunkY,
        accessedAt: ++accessSequence,
        modifications: new Map(),
      };
      chunks.set(key, chunk);
    }
    if (chunk) chunk.accessedAt = ++accessSequence;
    return chunk;
  };
  const evict = () => {
    if (chunks.size <= maximumResidentChunks) return;
    const removable = [...chunks.values()]
      .filter((chunk) => chunk.modifications.size === 0)
      .sort((a, b) => a.accessedAt - b.accessedAt);
    while (chunks.size > maximumResidentChunks && removable.length)
      chunks.delete(removable.shift().key);
  };

  const setModification = (x, y, patch) => {
    if (!Number.isInteger(x) || !Number.isInteger(y))
      throw new TypeError("Terrain modifications require integer cells.");
    const chunk = getChunk(x, y);
    const key = coordinateKey(x, y);
    if (!patch || Object.keys(patch).length === 0)
      chunk.modifications.delete(key);
    else chunk.modifications.set(key, structuredClone(patch));
    evict();
  };

  for (const modification of modifications)
    if (
      Number.isInteger(modification?.x) &&
      Number.isInteger(modification?.y) &&
      modification.patch
    )
      setModification(modification.x, modification.y, modification.patch);

  const sample = (x, y) => {
    const base = baseSampler(x, y);
    const cellX = Math.round(x);
    const cellY = Math.round(y);
    const chunk = getChunk(cellX, cellY);
    const patch = chunk.modifications.get(coordinateKey(cellX, cellY));
    evict();
    return patch ? { ...base, ...patch, modified: true } : base;
  };
  sample.terrain = baseSampler.terrain;
  sample.chunkSize = chunkSize;
  sample.setModification = setModification;
  sample.clearModification = (x, y) => setModification(x, y, null);
  sample.serializeModifications = () =>
    [...chunks.values()].flatMap((chunk) =>
      [...chunk.modifications].map(([key, patch]) => {
        const [x, y] = key.split(",").map(Number);
        return { x, y, patch: structuredClone(patch) };
      }),
    );
  sample.residentChunkCount = () => chunks.size;
  return sample;
}
