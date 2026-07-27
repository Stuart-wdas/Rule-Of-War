import { BUILDINGS, UNIT_TYPES } from "../game-rules.js";
import { planDirectCombat } from "../simulation/combat-resolution.js";
import { applyFormationDamage } from "../simulation/formation-state.js";

/** Resolves simultaneous combat while presenting attacks sequentially. */
export class CombatController {
  constructor({
    getState,
    getUnit,
    getAttackTarget,
    canUnitAttack,
    damageFor,
    getBattlefield,
    getLocalOwner,
    getEnemyOwner,
    coord,
    log,
    pause,
    modifyTerrain = null,
  }) {
    this.getState = getState;
    this.getUnit = getUnit;
    this.getAttackTarget = getAttackTarget;
    this.canUnitAttack = canUnitAttack;
    this.damageFor = damageFor;
    this.getBattlefield = getBattlefield;
    this.getLocalOwner = getLocalOwner;
    this.getEnemyOwner = getEnemyOwner;
    this.coord = coord;
    this.log = log;
    this.pause = pause;
    this.modifyTerrain = modifyTerrain;
  }

  async resolveAttacks(random = Math.random, { animate = true } = {}) {
    const state = this.getState();
    const plan = planDirectCombat({
      state,
      unitTypes: UNIT_TYPES,
      random,
      canAttack: (actor) => this.canUnitAttack(actor),
      damageFor: (actor) => this.damageFor(actor),
      resolveTarget: (order, actor) =>
        this.getAttackTarget(order.x, order.y, actor, order.targetId),
    });
    const results = plan.events.map(({ orderId, type: _type, ...event }) => ({
      ...event,
      id: orderId,
    }));
    const damage = plan.damageByTarget;
    const hitCounts = plan.hitCountByTarget;

    // Lifetime ammunition is consumed by firing, regardless of hit or miss.
    for (const [actorId, amount] of plan.ammunitionUsed) {
      const actor = this.getUnit(actorId);
      if (!actor) continue;
      actor.attackActionsUsed =
        (Number.isInteger(actor.attackActionsUsed)
          ? actor.attackActionsUsed
          : 0) + amount;
    }

    const battlefield = this.getBattlefield();
    const localOwner = this.getLocalOwner();
    if (animate && battlefield && results.length) {
      const projectileTimeoutMs = results.reduce(
        (total, result) =>
          total +
          (result.actorType === "xtralong"
            ? 3800
            : result.actorType === "long"
              ? 3200
              : 1700),
        1000,
      );
      await this.runAnimation(
        "projectile",
        () =>
          battlefield.animateAttacks(
            results.map((result) => ({
              ...result,
              owner: result.owner === localOwner ? 0 : 1,
            })),
          ),
        projectileTimeoutMs,
      );
    } else if (animate && results.length) {
      await this.pause(420);
    }

    for (const result of results) {
      const actor = this.getUnit(result.actorId);
      if (!actor) continue;
      const spec = UNIT_TYPES[actor.type];
      const actorName =
        actor.owner === localOwner ? spec.label : `HOSTILE ${spec.label}`;
      if (result.hit && result.targetId) {
        this.log(
          `${actorName} hit ${this.coord(result.to.x, result.to.y)} for ${result.damage} damage.`,
          actor.owner === this.getEnemyOwner(),
        );
      } else {
        this.log(
          `${actorName} fire missed ${this.coord(result.to.x, result.to.y)}.`,
          actor.owner === this.getEnemyOwner(),
        );
      }
    }

    // Apply accumulated damage only after the visual queue is finished.
    for (const [id, amount] of damage) {
      const target = [...state.units, ...state.structures].find(
        (item) => item.id === id,
      );
      if (target) {
        applyFormationDamage(target, amount, UNIT_TYPES);
        if ((hitCounts.get(id) || 0) > 1)
          this.log(
            `Concentrated fire dealt ${amount} damage at ${this.coord(target.x, target.y)}.`,
          );
      }
    }
    for (const result of results) {
      if (
        result.hit &&
        (result.actorType === "long" || result.actorType === "xtralong")
      )
        this.modifyTerrain?.(result.to.x, result.to.y, {
          crater: true,
          movementCostDelta: 1,
        });
    }

    const destroyedIds = [...state.units, ...state.structures]
      .filter((item) => item.hp <= 0)
      .map((item) => item.id);
    if (animate && battlefield && destroyedIds.length)
      await this.runAnimation(
        "destruction",
        () => battlefield.animateDestructions(destroyedIds),
        4500,
      );
    this.removeDestroyed();
    return results;
  }

  async runAnimation(label, animationFactory, timeoutMs) {
    try {
      await Promise.race([
        Promise.resolve().then(animationFactory),
        this.pause(timeoutMs).then(() => {
          throw new Error(`${label} animation timed out after ${timeoutMs}ms`);
        }),
      ]);
    } catch (error) {
      console.error(`The ${label} animation was skipped.`, error);
    }
  }

  removeDestroyed() {
    const state = this.getState();
    const localOwner = this.getLocalOwner();
    const deadUnits = state.units.filter((item) => item.hp <= 0);
    const deadStructures = state.structures.filter((item) => item.hp <= 0);
    for (const item of deadUnits) {
      const label =
        item.owner === localOwner
          ? UNIT_TYPES[item.type].label
          : `Hostile ${UNIT_TYPES[item.type].label}`;
      this.log(
        `${label} unit destroyed at ${this.coord(item.x, item.y)}.`,
        item.owner === localOwner,
      );
    }
    for (const item of deadStructures) {
      const label =
        item.owner === localOwner
          ? BUILDINGS[item.type].label
          : `Hostile ${BUILDINGS[item.type].label.toLowerCase()}`;
      this.log(
        `${label} destroyed at ${this.coord(item.x, item.y)}.`,
        item.owner === localOwner,
      );
    }
    state.units = state.units.filter((item) => item.hp > 0);
    state.structures = state.structures.filter((item) => item.hp > 0);
    const deadProducers = new Set(
      deadStructures
        .filter((item) => item.type === "barracks" || item.type === "airfield")
        .map((item) => item.id),
    );
    state.jobs = state.jobs.filter(
      (job) =>
        !deadProducers.has(job.producerId) &&
        !deadProducers.has(job.barracksId),
    );
  }
}
