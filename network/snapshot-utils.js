export function cloneJson(value) {
  return JSON.parse(JSON.stringify(value));
}

export function sortById(items) {
  return cloneJson(items).sort((a, b) =>
    String(a.id).localeCompare(String(b.id)),
  );
}

/** Creates the canonical state shape exchanged by multiplayer clients. */
export function buildNetworkSnapshot(state, config, configKeys) {
  const serializedConfig = {};
  for (const key of configKeys) serializedConfig[key] = config[key];
  return {
    schema: 1,
    config: serializedConfig,
    game: {
      round: state.round,
      phase: state.phase,
      winner: state.winner,
      objectiveControl: cloneJson(
        state.objectiveControl ?? { owner: null, rounds: 0 },
      ),
      objectiveWinner: state.objectiveWinner ?? null,
      players: cloneJson(state.players).sort((a, b) => a.id - b.id),
      structures: sortById(state.structures),
      units: sortById(state.units),
      jobs: sortById(state.jobs),
      orders: sortById(state.orders),
    },
  };
}

export function snapshotHash(snapshot) {
  const source = JSON.stringify(snapshot);
  let hash = 2166136261;
  for (let index = 0; index < source.length; index += 1) {
    hash ^= source.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

/** Small deterministic PRNG used so both multiplayer clients roll identically. */
export function seededRandom(seed) {
  let value = Number(seed) >>> 0;
  return () => {
    value += 0x6d2b79f5;
    let mixed = value;
    mixed = Math.imul(mixed ^ (mixed >>> 15), mixed | 1);
    mixed ^= mixed + Math.imul(mixed ^ (mixed >>> 7), mixed | 61);
    return ((mixed ^ (mixed >>> 14)) >>> 0) / 4294967296;
  };
}

export function normalizeMatchCode(value) {
  return String(value || "")
    .toUpperCase()
    .replace(/[^A-Z0-9]/g, "")
    .slice(0, 6);
}
