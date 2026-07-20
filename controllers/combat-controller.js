import { BUILDINGS, UNIT_TYPES } from "../game-rules.js";

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
  }

  async resolveAttacks(random = Math.random) {
    const state = this.getState();
    const attacks = state.orders.filter((item) => item.kind === "attack");
    const results = [];
    const damage = new Map();
    const hitCounts = new Map();
    const lifetimeShotsThisCombat = new Map();

    // Snapshot all attacks first. A unit destroyed this round still executes
    // the attack it had queued, preserving simultaneous combat semantics.
    for (const order of attacks) {
      const actor = this.getUnit(order.unitId);
      if (!actor || !this.canUnitAttack(actor)) continue;
      const spec = UNIT_TYPES[actor.type];
      if (Number.isInteger(spec.totalAttackActions)) {
        const alreadyUsed = Number.isInteger(actor.attackActionsUsed)
          ? actor.attackActionsUsed
          : 0;
        const queuedNow = lifetimeShotsThisCombat.get(actor.id) || 0;
        if (alreadyUsed + queuedNow >= spec.totalAttackActions) continue;
        lifetimeShotsThisCombat.set(actor.id, queuedNow + 1);
      }
      const target = this.getAttackTarget(order.x, order.y, actor);
      const hit = random() < spec.hitChance;
      const shotDamage = this.damageFor(actor);
      const targetId = target && target.owner !== actor.owner ? target.id : null;
      results.push({
        id: order.id,
        actorId: actor.id,
        actorType: actor.type,
        owner: actor.owner,
        from: { x: actor.x, y: actor.y },
        to: { x: order.x, y: order.y },
        hit,
        damage: shotDamage,
        targetId,
      });
      if (hit && targetId) {
        damage.set(targetId, (damage.get(targetId) || 0) + shotDamage);
        hitCounts.set(targetId, (hitCounts.get(targetId) || 0) + 1);
      }
    }

    // Lifetime ammunition is consumed by firing, regardless of hit or miss.
    for (const [actorId, amount] of lifetimeShotsThisCombat) {
      const actor = this.getUnit(actorId);
      if (!actor) continue;
      actor.attackActionsUsed =
        (Number.isInteger(actor.attackActionsUsed)
          ? actor.attackActionsUsed
          : 0) + amount;
    }

    const battlefield = this.getBattlefield();
    const localOwner = this.getLocalOwner();
    if (battlefield && results.length) {
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
    } else if (results.length) {
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
        target.hp -= amount;
        if ((hitCounts.get(id) || 0) > 1)
          this.log(
            `Concentrated fire dealt ${amount} damage at ${this.coord(target.x, target.y)}.`,
          );
      }
    }

    const destroyedIds = [...state.units, ...state.structures]
      .filter((item) => item.hp <= 0)
      .map((item) => item.id);
    if (battlefield && destroyedIds.length)
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
