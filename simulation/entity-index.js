/**
 * Read-optimized indexes over the serializable battle state.
 *
 * State remains plain arrays for persistence and network transport. Build this
 * index once per simulation or render pass instead of repeatedly scanning all
 * entities for id, owner, sector, and occupancy queries.
 */
export function createEntityIndex(state) {
  const units = Array.isArray(state?.units) ? state.units : [];
  const structures = Array.isArray(state?.structures) ? state.structures : [];
  const jobs = Array.isArray(state?.jobs) ? state.jobs : [];
  const orders = Array.isArray(state?.orders) ? state.orders : [];
  const entities = [...units, ...structures];
  const byId = new Map(entities.map((entity) => [entity.id, entity]));
  const unitsById = new Map(units.map((unit) => [unit.id, unit]));
  const structuresById = new Map(
    structures.map((structure) => [structure.id, structure]),
  );
  const byOwner = new Map();
  const bySector = new Map();
  const occupancy = new Map();
  const ordersByUnit = new Map();

  const append = (map, key, value) => {
    const values = map.get(key) ?? [];
    values.push(value);
    map.set(key, values);
  };
  const coordinatesFor = (entity) =>
    Array.isArray(entity.footprint) && entity.footprint.length
      ? entity.footprint
      : [{ x: entity.x, y: entity.y }];

  for (const entity of entities) {
    append(byOwner, entity.owner, entity);
    append(bySector, entity.sectorId ?? state.sectorId ?? "battle", entity);
    for (const point of coordinatesFor(entity))
      occupancy.set(`${point.x},${point.y}`, entity);
  }
  for (const order of orders) append(ordersByUnit, order.unitId, order);

  return {
    entities,
    byId,
    unitsById,
    structuresById,
    byOwner,
    bySector,
    occupancy,
    ordersByUnit,
    get(id) {
      return byId.get(id) ?? null;
    },
    unit(id) {
      return unitsById.get(id) ?? null;
    },
    structure(id) {
      return structuresById.get(id) ?? null;
    },
    at(x, y) {
      return occupancy.get(`${x},${y}`) ?? null;
    },
    owned(owner) {
      return byOwner.get(owner) ?? [];
    },
    inSector(sectorId) {
      return bySector.get(sectorId) ?? [];
    },
    ordersFor(unitId) {
      return ordersByUnit.get(unitId) ?? [];
    },
  };
}
