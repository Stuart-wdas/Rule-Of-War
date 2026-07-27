import { UNIT_TYPES } from "../game-rules.js";
import { applyFormationDamage } from "../simulation/formation-state.js";

function distanceToSegment(point, start, end) {
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  const lengthSquared = dx * dx + dy * dy;
  if (lengthSquared === 0) return Math.hypot(point.x - start.x, point.y - start.y);
  const progress = Math.max(
    0,
    Math.min(
      1,
      ((point.x - start.x) * dx + (point.y - start.y) * dy) /
        lengthSquared,
    ),
  );
  return Math.hypot(
    point.x - (start.x + dx * progress),
    point.y - (start.y + dy * progress),
  );
}

/**
 * Resolves bomber missions after outbound movement and before normal fire.
 *
 * A sortie is deliberately one order: take off, cross interception ranges,
 * release one payload over the destination tile, then return to its Airfield.
 */
export class AirController {
  constructor({
    getState,
    getBattlefield,
    getWorldPoint,
    getAttackTarget,
    damageFor,
    getLocalOwner,
    coord,
    log,
  }) {
    this.getState = getState;
    this.getBattlefield = getBattlefield;
    this.getWorldPoint = getWorldPoint;
    this.getAttackTarget = getAttackTarget;
    this.damageFor = damageFor;
    this.getLocalOwner = getLocalOwner;
    this.coord = coord;
    this.log = log;
  }

  async resolveSorties(random = Math.random, { animate = true } = {}) {
    const state = this.getState();
    const localOwner = this.getLocalOwner();
    const bombers = state.units.filter(
      (unit) =>
        UNIT_TYPES[unit.type]?.airUnit === true &&
        unit.airborne === true &&
        unit.sortie,
    );
    if (!bombers.length) return [];

    const battlefield = this.getBattlefield();
    const interceptionResults = [];
    for (const bomber of bombers) {
      const start = this.getWorldPoint(bomber.sortie.from);
      const end = this.getWorldPoint(bomber.sortie.target);
      for (const interceptor of state.units) {
        if (
          interceptor.owner === bomber.owner ||
          interceptor.hp <= 0 ||
          UNIT_TYPES[interceptor.type]?.airUnit === true
        )
          continue;
        const spec = UNIT_TYPES[interceptor.type];
        if (!spec || spec.antiAir !== true) continue;
        const lifetimeLimit = spec.totalAttackActions;
        const alreadyUsed = Number.isInteger(interceptor.attackActionsUsed)
          ? interceptor.attackActionsUsed
          : 0;
        if (Number.isInteger(lifetimeLimit)) {
          const reservedShots = state.orders.filter(
            (order) =>
              order.kind === "attack" && order.unitId === interceptor.id,
          ).length;
          if (alreadyUsed + reservedShots >= lifetimeLimit) continue;
        }
        const interceptorPoint = this.getWorldPoint(interceptor);
        if (distanceToSegment(interceptorPoint, start, end) > spec.range)
          continue;
        if (Number.isInteger(lifetimeLimit))
          interceptor.attackActionsUsed = alreadyUsed + 1;
        interceptionResults.push({
          actorId: interceptor.id,
          actorType: interceptor.type,
          owner: interceptor.owner,
          from: { x: interceptor.x, y: interceptor.y },
          to: { x: bomber.x, y: bomber.y },
          targetId: bomber.id,
          hit: random() < spec.hitChance,
          damage: this.damageFor(interceptor),
          targetAltitude: 1.72,
        });
      }
    }

    if (animate && battlefield && interceptionResults.length)
      await battlefield.animateInterceptions(
        interceptionResults.map((result) => ({
          ...result,
          owner: result.owner === localOwner ? 0 : 1,
        })),
      );

    for (const result of interceptionResults) {
      const interceptor = state.units.find((unit) => unit.id === result.actorId);
      const bomber = state.units.find((unit) => unit.id === result.targetId);
      if (!interceptor || !bomber) continue;
      const interceptorName =
        interceptor.owner === localOwner
          ? UNIT_TYPES[interceptor.type].label
          : `HOSTILE ${UNIT_TYPES[interceptor.type].label}`;
      if (result.hit) {
        applyFormationDamage(bomber, result.damage, UNIT_TYPES);
        this.log(
          `${interceptorName} interception hit ${UNIT_TYPES[bomber.type].label} for ${result.damage} damage.`,
          bomber.owner === localOwner,
        );
      } else {
        this.log(
          `${interceptorName} interception fire missed the bomber.`,
          interceptor.owner !== localOwner,
        );
      }
    }

    const shotDown = bombers.filter((bomber) => bomber.hp <= 0);
    if (animate && battlefield && shotDown.length)
      await battlefield.animateDestructions(shotDown.map((bomber) => bomber.id));
    for (const bomber of shotDown)
      this.log(
        `${bomber.owner === localOwner ? "BOMBER" : "Hostile bomber"} shot down over ${this.coord(bomber.x, bomber.y)} before releasing its payload.`,
        bomber.owner === localOwner,
      );
    const shotDownIds = new Set(shotDown.map((bomber) => bomber.id));
    state.units = state.units.filter((unit) => !shotDownIds.has(unit.id));
    if (shotDownIds.has(state.selectedId)) state.selectedId = null;
    if (Array.isArray(state.selectedIds))
      state.selectedIds = state.selectedIds.filter((id) => !shotDownIds.has(id));

    const survivors = bombers.filter((bomber) => !shotDownIds.has(bomber.id));
    const bombResults = survivors.map((bomber) => {
      const target = this.getAttackTarget(bomber.x, bomber.y, bomber);
      return {
        actorId: bomber.id,
        actorType: bomber.type,
        owner: bomber.owner,
        from: { x: bomber.x, y: bomber.y },
        to: { x: bomber.x, y: bomber.y },
        targetId: target?.id ?? null,
        hit: random() < UNIT_TYPES[bomber.type].hitChance,
        damage: this.damageFor(bomber),
      };
    });

    if (animate && battlefield && bombResults.length)
      await battlefield.animateBomberBombs(
        bombResults.map((result) => ({
          ...result,
          owner: result.owner === localOwner ? 0 : 1,
        })),
      );

    for (const result of bombResults) {
      const bomber = state.units.find((unit) => unit.id === result.actorId);
      const target = [...state.units, ...state.structures].find(
        (piece) => piece.id === result.targetId,
      );
      if (!bomber) continue;
      const bomberName = bomber.owner === localOwner ? "BOMBER" : "HOSTILE BOMBER";
      if (result.hit && target) {
        applyFormationDamage(target, result.damage, UNIT_TYPES);
        this.log(
          `${bomberName} dropped its payload on ${this.coord(result.to.x, result.to.y)} for ${result.damage} damage.`,
          bomber.owner !== localOwner,
        );
      } else {
        this.log(
          `${bomberName} payload missed at ${this.coord(result.to.x, result.to.y)}.`,
          bomber.owner !== localOwner,
        );
      }
    }

    const returns = [];
    const stranded = [];
    for (const bomber of survivors) {
      const airfield = state.structures.find(
        (structure) =>
          structure.id === bomber.homeAirfieldId &&
          structure.type === "airfield" &&
          structure.hp > 0,
      );
      if (!airfield) {
        stranded.push(bomber);
        continue;
      }
      returns.push({
        id: bomber.id,
        from: { x: bomber.x, y: bomber.y },
        to: {
          x: airfield.x,
          y: airfield.y,
          worldX:
            airfield.x + [0.1, 1, 1.9][bomber.parkingSlot ?? 0],
          worldY: airfield.y + 0.8,
        },
      });
    }
    if (animate && battlefield && returns.length)
      await battlefield.animateBomberReturns(returns);
    for (const mission of returns) {
      const bomber = state.units.find((unit) => unit.id === mission.id);
      if (!bomber) continue;
      bomber.x = mission.to.x;
      bomber.y = mission.to.y;
      bomber.worldX = mission.to.worldX;
      bomber.worldY = mission.to.worldY;
      bomber.airborne = false;
      bomber.sortie = null;
      this.log(
        `${bomber.owner === localOwner ? "BOMBER" : "Hostile bomber"} returned to its Airfield.`,
        false,
      );
    }
    if (stranded.length) {
      if (animate && battlefield)
        await battlefield.animateDestructions(
          stranded.map((bomber) => bomber.id),
        );
      const strandedIds = new Set(stranded.map((bomber) => bomber.id));
      state.units = state.units.filter((unit) => !strandedIds.has(unit.id));
      if (strandedIds.has(state.selectedId)) state.selectedId = null;
      if (Array.isArray(state.selectedIds))
        state.selectedIds = state.selectedIds.filter(
          (id) => !strandedIds.has(id),
        );
      for (const bomber of stranded)
        this.log(
          `${bomber.owner === localOwner ? "BOMBER" : "Hostile bomber"} was lost because its Airfield was unavailable.`,
          bomber.owner === localOwner,
        );
    }
    return bombResults;
  }
}
