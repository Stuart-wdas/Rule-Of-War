import { BUILDINGS, UNIT_TYPES } from "../game-rules.js";

/**
 * A small, rule-based opponent controller.
 *
 * Engine operations are injected so this controller decides what to do while
 * game.js remains responsible for validating, queuing, and resolving orders.
 * Pass a seeded `random` function in tests to make its choices repeatable.
 */
export class AiController {
  constructor({
    owner = 1,
    getState,
    getGameConfig,
    getNavigableTiles,
    getTilesInRange,
    distance,
    isValidBuildTile,
    isValidMove,
    queueBuilding,
    queueRecruit,
    queueOrder,
    canUnitAttack,
    attackActionLimitFor,
    random = Math.random,
  }) {
    this.owner = owner;
    this.getState = getState;
    this.getGameConfig = getGameConfig;
    this.getNavigableTiles = getNavigableTiles;
    this.getTilesInRange = getTilesInRange;
    this.distance = distance;
    this.isValidBuildTile = isValidBuildTile;
    this.isValidMove = isValidMove;
    this.queueBuilding = queueBuilding;
    this.queueRecruit = queueRecruit;
    this.queueOrder = queueOrder;
    this.canUnitAttack = canUnitAttack;
    this.attackActionLimitFor = attackActionLimitFor;
    this.random = random;
  }

  /** Plans economy choices and movement before the movement queue resolves. */
  planDeploymentAndMoves() {
    const state = this.getState();
    const config = this.getGameConfig();
    const owner = this.owner;
    const enemy = state.players[owner];
    const prefix = owner === 0 ? "player" : "enemy";
    const mineCount =
      state.structures.filter(
        (item) => item.owner === owner && item.type === "mine",
      ).length +
      state.jobs.filter(
        (item) => item.owner === owner && item.type === "mine",
      ).length;
    const airfieldCount =
      state.structures.filter(
        (item) => item.owner === owner && item.type === "airfield",
      ).length +
      state.jobs.filter(
        (item) =>
          item.owner === owner &&
          item.kind === "building" &&
          item.type === "airfield",
      ).length;
    const bomberCount =
      state.units.filter(
        (item) => item.owner === owner && UNIT_TYPES[item.type]?.airUnit,
      ).length +
      state.jobs.filter(
        (item) =>
          item.owner === owner &&
          item.kind === "recruit" &&
          UNIT_TYPES[item.type]?.airUnit,
      ).length;

    if (mineCount < 1 && enemy.resources >= BUILDINGS.mine.price) {
      const tile = this.findBuildTile();
      if (tile) this.queueBuilding("mine", tile.x, tile.y, owner);
    } else if (
      airfieldCount < 1 &&
      enemy.resources >= BUILDINGS.airfield.price
    ) {
      const tile = this.findBuildTile();
      if (tile) this.queueBuilding("airfield", tile.x, tile.y, owner);
    } else if (
      airfieldCount > 0 &&
      bomberCount < airfieldCount &&
      enemy.resources >= UNIT_TYPES.plane.price
    ) {
      this.queueRecruit("plane", owner);
    } else if (
      enemy.resources >= UNIT_TYPES.short.price &&
      state.structures.some(
        (item) => item.owner === owner && item.type === "barracks",
      ) &&
      state.units.filter((item) => item.owner === owner).length <
        Math.max(
          5,
          config[`${prefix}Short`] +
            config[`${prefix}Medium`] +
            config[`${prefix}Long`] +
            config[`${prefix}Xtralong`] +
            2,
        )
    ) {
      this.queueRecruit(this.chooseRecruitType(enemy.resources), owner);
    }

    const targets = this.enemyPieces().filter(
      (piece) => UNIT_TYPES[piece.type]?.airUnit !== true,
    );
    for (const actor of state.units.filter((item) => item.owner === owner)) {
      const spec = UNIT_TYPES[actor.type];
      if (enemy.resources < spec.actionCost || !targets.length) continue;
      const nearest = [...targets].sort(
        (a, b) => this.distance(actor, a) - this.distance(actor, b),
      )[0];
      if (spec.airUnit === true) {
        if (
          actor.airborne !== true &&
          this.distance(actor, nearest) <= spec.movementRange
        )
          this.queueOrder(actor, "move", nearest.x, nearest.y, owner);
        continue;
      }
      if (this.distance(actor, nearest) > spec.range) {
        const step = this.bestStepToward(actor, nearest);
        if (step) this.queueOrder(actor, "move", step.x, step.y, owner);
      }
    }
  }

  /** Queues attacks for every stationary AI unit before combat resolution. */
  planAttacks() {
    const state = this.getState();
    const owner = this.owner;
    const enemy = state.players[owner];
    const targets = this.enemyPieces().filter(
      (piece) => UNIT_TYPES[piece.type]?.airUnit !== true,
    );
    if (!targets.length) return;

    for (const actor of state.units.filter((item) => item.owner === owner)) {
      if (!this.canUnitAttack(actor)) continue;
      const spec = UNIT_TYPES[actor.type];
      if (enemy.resources < spec.actionCost) continue;
      const inRange = targets
        .filter((target) => this.distance(actor, target) <= spec.range)
        .sort((a, b) => this.distance(actor, a) - this.distance(actor, b));
      for (
        let index = 0;
        index < this.attackActionLimitFor(actor);
        index += 1
      ) {
        const target = inRange[index % inRange.length];
        if (
          !target ||
          !this.queueOrder(actor, "attack", target.x, target.y, owner)
        )
          break;
      }
    }
  }

  chooseRecruitType(resources) {
    const roll = this.random();
    if (resources >= UNIT_TYPES.xtralong.price && roll < 0.1)
      return "xtralong";
    if (resources >= UNIT_TYPES.long.price && roll < 0.32) return "long";
    if (resources >= UNIT_TYPES.medium.price && roll < 0.62) return "medium";
    return "short";
  }

  enemyPieces() {
    const state = this.getState();
    return [...state.units, ...state.structures].filter(
      (item) => item.owner !== this.owner,
    );
  }

  findBuildTile() {
    const state = this.getState();
    const sources = [
      ...state.structures
        .filter((piece) => piece.owner === this.owner)
        .map((piece) => ({ piece, range: 4 })),
      ...state.units
        .filter(
          (piece) => piece.owner === this.owner && piece.type === "commander",
        )
        .map((piece) => ({
          piece,
          range:
            piece.movementRange ?? UNIT_TYPES[piece.type].movementRange,
        })),
    ];
    const nearby = new Map();
    for (const { piece, range } of sources)
      for (const tile of this.getTilesInRange?.(piece, range) ?? [])
        nearby.set(`${tile.x},${tile.y}`, tile);
    const searchTiles = nearby.size
      ? [...nearby.values()]
      : this.getNavigableTiles();
    const candidates = searchTiles.filter((tile) =>
      this.isValidBuildTile(tile.x, tile.y, this.owner),
    );
    return candidates[Math.floor(this.random() * candidates.length)] || null;
  }

  bestStepToward(actor, target) {
    const movementRange =
      actor.movementRange ?? UNIT_TYPES[actor.type].movementRange;
    const searchTiles =
      this.getTilesInRange?.(actor, movementRange) ?? this.getNavigableTiles();
    const candidates = searchTiles.filter((tile) =>
      this.isValidMove(actor, tile.x, tile.y),
    );
    candidates.sort(
      (a, b) => this.distance(a, target) - this.distance(b, target),
    );
    return candidates[0] || null;
  }
}
