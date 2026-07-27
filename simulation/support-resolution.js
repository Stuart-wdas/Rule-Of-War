import { createEntityIndex } from "./entity-index.js";
import { restoreFormationStrength } from "./formation-state.js";

/** Automatically heals one nearby friendly formation per medical unit. */
export function resolveMedicalSupport(
  state,
  unitTypes,
  {
    buildingTypes = {},
    distance = (a, b) => Math.hypot(a.x - b.x, a.y - b.y),
  } = {},
) {
  const index = createEntityIndex(state);
  const events = [];
  const reserved = new Set();
  const medics = [
    ...state.units.filter(
      (unit) => unit.hp > 0 && unitTypes[unit.type]?.medical,
    ),
    ...state.structures.filter(
      (structure) =>
        structure.hp > 0 && buildingTypes[structure.type]?.medicalFacility,
    ),
  ]
    .sort((a, b) => String(a.id).localeCompare(String(b.id)));

  for (const medic of medics) {
    const spec = unitTypes[medic.type] ?? buildingTypes[medic.type];
    const target = index
      .owned(medic.owner)
      .filter(
        (entity) =>
          index.unitsById.has(entity.id) &&
          entity.id !== medic.id &&
          entity.hp > 0 &&
          entity.hp < (unitTypes[entity.type]?.lives ?? entity.hp) &&
          distance(medic, entity) <= (spec.supportRange ?? 3) &&
          !reserved.has(entity.id),
      )
      .sort(
        (a, b) =>
          a.hp - b.hp ||
          distance(medic, a) - distance(medic, b) ||
          String(a.id).localeCompare(String(b.id)),
      )[0];
    if (!target) continue;

    const maximum = unitTypes[target.type]?.lives ?? target.hp;
    const amount = Math.min(spec.healing ?? 1, maximum - target.hp);
    target.hp += amount;
    restoreFormationStrength(target, amount, unitTypes);
    reserved.add(target.id);
    events.push({
      type: "unit-healed",
      medicId: medic.id,
      sourceId: medic.id,
      sourceType: medic.type,
      targetId: target.id,
      owner: medic.owner,
      amount,
      x: target.x,
      y: target.y,
    });
  }
  return events;
}
