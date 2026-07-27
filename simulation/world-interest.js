import { neighboringSectorIds } from "./world-state.js";

/**
 * Produces an interest-managed strategic view.
 * Owned and adjacent sectors retain tactical metadata; distant sectors expose
 * only public control and conflict information.
 */
export function worldViewForFaction(world, factionId) {
  const detailedIds = new Set();
  for (const sector of world.sectors) {
    if (sector.owner !== factionId) continue;
    detailedIds.add(sector.id);
    for (const neighborId of neighboringSectorIds(world, sector.id))
      detailedIds.add(neighborId);
  }
  return {
    schema: world.schema,
    id: world.id,
    turn: world.turn,
    width: world.width,
    height: world.height,
    factions: structuredClone(world.factions),
    sectors: world.sectors.map((sector) =>
      detailedIds.has(sector.id)
        ? structuredClone(sector)
        : {
            id: sector.id,
            x: sector.x,
            y: sector.y,
            owner: sector.owner,
            contested: sector.contested,
          },
    ),
    armies: (world.armies ?? [])
      .filter(
        (army) =>
          army.owner === factionId || detailedIds.has(army.sectorId),
      )
      .map((army) => structuredClone(army)),
  };
}
