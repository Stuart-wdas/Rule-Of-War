const DEFAULT_BIOMES = ["plains", "forest", "highlands", "coast"];

/**
 * Creates a compact strategic layer. Each sector owns a deterministic tactical
 * seed, so the world map never needs to carry full battlefield terrain.
 */
export function createWorldState({
  width = 10,
  height = 4,
  factionCount = 2,
  seed = "rule-of-war",
} = {}) {
  const sectors = [];
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const id = `sector-${x}-${y}`;
      const homeRow = Math.floor(height / 2);
      const edgeOwner =
        x === 0 && y === homeRow
          ? 0
          : x === width - 1 && y === homeRow
            ? factionCount - 1
            : null;
      sectors.push({
        id,
        x,
        y,
        name: `SECTOR ${String(y * width + x + 1).padStart(2, "0")}`,
        biome: DEFAULT_BIOMES[(x * 3 + y * 5) % DEFAULT_BIOMES.length],
        terrainSeed: `${seed}:${x}:${y}`,
        owner: edgeOwner,
        objective: (x + y) % 3 === 0 ? "settlement" : "command-post",
        supply: edgeOwner === null ? 0 : 1,
        contested: false,
        battleId: null,
        modifications: [],
      });
    }
  }
  return {
    schema: 3,
    id: `world-${seed}`,
    seed,
    turn: 1,
    width,
    height,
    factions: Array.from({ length: factionCount }, (_, id) => ({
      id,
      name: id === 0 ? "FIELD COMMAND" : `FRONT ${id + 1}`,
      color: id === 0 ? "#5fa87a" : "#b55f55",
    })),
    sectors,
    armies: [],
    events: [],
  };
}

export function neighboringSectorIds(world, sectorId) {
  const sector = world.sectors.find((item) => item.id === sectorId);
  if (!sector) return [];
  const diagonalOffset = sector.y % 2 === 0 ? -1 : 1;
  return [
    [sector.x - 1, sector.y],
    [sector.x + 1, sector.y],
    [sector.x, sector.y - 1],
    [sector.x, sector.y + 1],
    [sector.x + diagonalOffset, sector.y - 1],
    [sector.x + diagonalOffset, sector.y + 1],
  ]
    .map(([x, y]) =>
      world.sectors.find((item) => item.x === x && item.y === y),
    )
    .filter(Boolean)
    .map((item) => item.id);
}

export function canFactionContestSector(world, sectorId, factionId) {
  const sector = world.sectors.find((item) => item.id === sectorId);
  if (!sector || sector.owner === factionId) return false;
  return neighboringSectorIds(world, sectorId).some(
    (neighborId) =>
      world.sectors.find((item) => item.id === neighborId)?.owner === factionId,
  );
}

/** Marks ownership chains as supplied without inventories or convoy controls. */
export function recomputeWorldSupply(world) {
  for (const sector of world.sectors) sector.supply = 0;
  for (const faction of world.factions) {
    const homeX = faction.id === 0 ? 0 : world.width - 1;
    const queue = world.sectors.filter(
      (sector) => sector.owner === faction.id && sector.x === homeX,
    );
    const supplied = new Set(queue.map((sector) => sector.id));
    for (let cursor = 0; cursor < queue.length; cursor += 1) {
      const sector = queue[cursor];
      sector.supply = 1;
      for (const neighborId of neighboringSectorIds(world, sector.id)) {
        const neighbor = world.sectors.find((item) => item.id === neighborId);
        if (
          neighbor?.owner === faction.id &&
          !supplied.has(neighbor.id)
        ) {
          supplied.add(neighbor.id);
          queue.push(neighbor);
        }
      }
    }
  }
  return world;
}

export function applySectorOutcome(world, sectorId, winner) {
  const sector = world.sectors.find((item) => item.id === sectorId);
  if (!sector || !world.factions.some((faction) => faction.id === winner))
    return null;
  const previousOwner = sector.owner;
  sector.owner = winner;
  sector.contested = false;
  sector.battleId = null;
  recomputeWorldSupply(world);
  const event = {
    type: "sector-captured",
    turn: world.turn,
    sectorId,
    previousOwner,
    owner: winner,
  };
  world.events.push(event);
  return event;
}
