/**
 * Builds the read-only indexes needed for one battlefield render.
 *
 * Gameplay state remains serializable arrays. The view turns those arrays into
 * maps and spatial sets once, avoiding repeated find/filter calls per tile.
 */
export function createBattlefieldViewIndex({
  state,
  runtimeMap,
  localOwner,
  fogEnabled,
  unitTypes,
  buildings,
  selected,
  movementRangeFor,
  getWorldPoint = null,
  canUnitAttack,
  isValidBuildTile,
  isTerrainPassable,
}) {
  const keyOf = (x, y) => `${x},${y}`;
  const occupiedCoordinates = (piece) =>
    Array.isArray(piece.footprint) && piece.footprint.length
      ? piece.footprint
      : [{ x: piece.x, y: piece.y }];
  const occupancy = new Map();
  const blockingOccupancy = new Set();
  for (const piece of state.structures) {
    for (const tile of occupiedCoordinates(piece)) {
      occupancy.set(keyOf(tile.x, tile.y), piece);
      if (piece.type !== "bridge") blockingOccupancy.add(keyOf(tile.x, tile.y));
    }
  }
  for (const piece of state.units) {
    occupancy.set(keyOf(piece.x, piece.y), piece);
    if (
      unitTypes[piece.type]?.airUnit !== true ||
      piece.airborne !== true
    )
      blockingOccupancy.add(keyOf(piece.x, piece.y));
  }
  for (const job of state.jobs) {
    if (job.kind !== "building") continue;
    for (const tile of occupiedCoordinates(job)) {
      const key = keyOf(tile.x, tile.y);
      if (!occupancy.has(key)) occupancy.set(key, job);
      if (job.type !== "bridge") blockingOccupancy.add(key);
    }
  }

  const orderByUnit = new Map();
  const fireCountByTile = new Map();
  for (const order of state.orders) {
    if (!orderByUnit.has(order.unitId)) orderByUnit.set(order.unitId, order);
    if (order.owner === localOwner && order.kind === "attack") {
      const key = keyOf(order.x, order.y);
      fireCountByTile.set(key, (fireCountByTile.get(key) ?? 0) + 1);
    }
  }

  const usesAnalyticVisibility = fogEnabled && runtimeMap.virtual;
  const visible = fogEnabled && !usesAnalyticVisibility ? new Set() : null;
  const ownedVisible = fogEnabled ? new Set() : null;
  // The renderer consumes the same reveal sources as the gameplay visibility
  // index. Keeping them here prevents the visual fog radius from drifting away
  // from the range rule used to reveal units and tiles.
  const visibilitySources = [];
  if (fogEnabled) {
    for (const piece of [...state.units, ...state.structures]) {
      if (piece.owner !== localOwner) continue;
      for (const tile of occupiedCoordinates(piece))
        ownedVisible.add(keyOf(tile.x, tile.y));
      if (piece.revealsFog !== true) continue;
      const spec = unitTypes[piece.type] ?? buildings[piece.type];
      if (!spec) continue;
      // Bombers only act as reconnaissance sources while airborne. A parked
      // aircraft remains visible to its owner but does not clear surrounding
      // fog or reveal hostile units around its Airfield.
      if (spec.airUnit === true && piece.airborne !== true) continue;
      const visionRange = spec.visionRange ?? spec.range ?? 0;
      const center =
        getWorldPoint?.(piece) ??
        runtimeMap.tileByCoordinate.get(keyOf(piece.x, piece.y))?.center ??
        { x: piece.x, y: piece.y };
      visibilitySources.push({
        id: piece.id,
        x: center.x,
        y: center.y,
        radius: visionRange,
      });
      if (visible)
        for (const tile of runtimeMap.tilesWithinRadius(piece, visionRange, {
          includeOrigin: true,
        }))
          visible.add(keyOf(tile.x, tile.y));
    }
    for (const job of state.jobs)
      if (job.owner === localOwner && job.kind === "building") {
        for (const tile of occupiedCoordinates(job))
          ownedVisible.add(keyOf(tile.x, tile.y));
      }
  }

  const available = new Set();
  if (state.buildMode) {
    const buildSpec = buildings[state.buildMode];
    if (!buildSpec?.commanderOnly)
      for (const structure of state.structures) {
        if (structure.owner !== localOwner) continue;
        const buildRange = buildings[structure.type]?.buildRange ?? 2;
        for (const tile of runtimeMap.tilesWithinRadius(structure, buildRange))
          if (isValidBuildTile?.(tile.x, tile.y, localOwner))
            available.add(keyOf(tile.x, tile.y));
      }
    for (const commander of state.units) {
      if (
        commander.owner !== localOwner ||
        (commander.type !== "commander" &&
          unitTypes[commander.type]?.engineer !== true) ||
        (state.bridgeStart?.commanderId &&
          commander.id !== state.bridgeStart.commanderId)
      )
        continue;
      for (const tile of runtimeMap.tilesWithinRadius(
        commander,
        movementRangeFor(commander),
      )) {
        const key = keyOf(tile.x, tile.y);
        if (
          (!isValidBuildTile ||
            isValidBuildTile(tile.x, tile.y, localOwner)) &&
          runtimeMap.graph.isConnected(commander, tile)
        )
          available.add(key);
      }
    }
  }

  const reachable = new Set();
  if (selected && state.intent === "move") {
    for (const tile of runtimeMap.tilesWithinRadius(
      selected,
      movementRangeFor(selected),
    )) {
      const key = keyOf(tile.x, tile.y);
      if (
        (unitTypes[selected.type]?.airUnit === true ||
          (!blockingOccupancy.has(key) &&
            (!isTerrainPassable || isTerrainPassable(tile.x, tile.y)))) &&
        runtimeMap.graph.isConnected(selected, tile)
      )
        reachable.add(key);
    }
  }

  const targetable = new Set();
  if (selected && state.intent === "attack" && canUnitAttack(selected)) {
    for (const tile of runtimeMap.tilesWithinRadius(
      selected,
      unitTypes[selected.type].range,
    ))
      targetable.add(keyOf(tile.x, tile.y));
  }

  return {
    occupancy,
    orderByUnit,
    fireCountByTile,
    available,
    reachable,
    targetable,
    visibilitySources,
    isVisible(x, y) {
      if (!fogEnabled) return true;
      const key = keyOf(x, y);
      if (ownedVisible.has(key) || visible?.has(key)) return true;
      if (!usesAnalyticVisibility) return false;
      const center = runtimeMap.tileByCoordinate.get(key)?.center ?? { x, y };
      return visibilitySources.some(
        (source) =>
          (center.x - source.x) ** 2 + (center.y - source.y) ** 2 <=
          source.radius ** 2,
      );
    },
  };
}
