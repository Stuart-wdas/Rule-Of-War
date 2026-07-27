import { createEntityIndex } from "./entity-index.js";

/**
 * Plans simultaneous direct-fire combat without mutating state.
 *
 * Target lookup is injected because authored maps may use footprints and
 * precise world-space targets. The resulting event list is presentation-free
 * and can be consumed by the browser, a replay, or an authoritative server.
 */
export function planDirectCombat({
  state,
  unitTypes,
  random,
  resolveTarget,
  canAttack = () => true,
  damageFor = (unit) => unit.damage ?? unitTypes[unit.type]?.damage ?? 0,
}) {
  const index = createEntityIndex(state);
  const events = [];
  const damageByTarget = new Map();
  const hitCountByTarget = new Map();
  const ammunitionUsed = new Map();

  for (const order of state.orders.filter((item) => item.kind === "attack")) {
    const actor = index.unit(order.unitId);
    if (!actor || actor.hp <= 0 || !canAttack(actor, order)) continue;
    const spec = unitTypes[actor.type];
    if (!spec) continue;

    const alreadyUsed = Number.isInteger(actor.attackActionsUsed)
      ? actor.attackActionsUsed
      : 0;
    const queuedNow = ammunitionUsed.get(actor.id) ?? 0;
    if (
      Number.isInteger(spec.totalAttackActions) &&
      alreadyUsed + queuedNow >= spec.totalAttackActions
    )
      continue;

    const target =
      order.preciseTarget && !order.targetId
        ? null
        : resolveTarget(order, actor, index);
    const hit = random() < spec.hitChance;
    const damage = damageFor(actor);
    const targetId =
      target && target.hp > 0 && target.owner !== actor.owner
        ? target.id
        : null;

    ammunitionUsed.set(actor.id, queuedNow + 1);
    events.push({
      type: "attack-resolved",
      orderId: order.id,
      actorId: actor.id,
      actorType: actor.type,
      owner: actor.owner,
      from: { x: actor.x, y: actor.y },
      to: {
        x: order.x,
        y: order.y,
        worldX: order.worldX,
        worldY: order.worldY,
      },
      hit,
      damage,
      targetId,
    });
    if (hit && targetId) {
      damageByTarget.set(
        targetId,
        (damageByTarget.get(targetId) ?? 0) + damage,
      );
      hitCountByTarget.set(
        targetId,
        (hitCountByTarget.get(targetId) ?? 0) + 1,
      );
    }
  }

  return { events, damageByTarget, hitCountByTarget, ammunitionUsed };
}

/** Applies a planned result and returns the removed entities as events. */
export function applyDirectCombat(state, plan) {
  const index = createEntityIndex(state);
  for (const [actorId, amount] of plan.ammunitionUsed) {
    const actor = index.unit(actorId);
    if (actor)
      actor.attackActionsUsed =
        (Number.isInteger(actor.attackActionsUsed)
          ? actor.attackActionsUsed
          : 0) + amount;
  }
  for (const [targetId, amount] of plan.damageByTarget) {
    const target = index.get(targetId);
    if (target) target.hp -= amount;
  }

  const destroyed = index.entities
    .filter((entity) => entity.hp <= 0)
    .map((entity) => ({
      type: "entity-destroyed",
      entityId: entity.id,
      entityType: entity.type,
      owner: entity.owner,
      x: entity.x,
      y: entity.y,
      entityKind: index.unitsById.has(entity.id) ? "unit" : "structure",
    }));
  const destroyedIds = new Set(destroyed.map((event) => event.entityId));
  state.units = state.units.filter((unit) => !destroyedIds.has(unit.id));
  state.structures = state.structures.filter(
    (structure) => !destroyedIds.has(structure.id),
  );

  const destroyedProducerIds = new Set(
    destroyed
      .filter(
        (event) =>
          event.entityKind === "structure" &&
          (event.entityType === "barracks" || event.entityType === "airfield"),
      )
      .map((event) => event.entityId),
  );
  state.jobs = state.jobs.filter(
    (job) =>
      !destroyedProducerIds.has(job.producerId) &&
      !destroyedProducerIds.has(job.barracksId),
  );
  return destroyed;
}
