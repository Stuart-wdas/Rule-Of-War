import { UNIT_TYPES } from "../game-rules.js";

/** Owns movement collision checks, presentation, and final position updates. */
export class MovementController {
  constructor({
    getUnit,
    pieceAt,
    isGroundOccupied,
    getBattlefield,
    isVisible,
    getLocalOwner,
    getEnemyOwner,
    coord,
    log,
    pause,
  }) {
    this.getUnit = getUnit;
    this.pieceAt = pieceAt;
    this.isGroundOccupied = isGroundOccupied;
    this.getBattlefield = getBattlefield;
    this.isVisible = isVisible;
    this.getLocalOwner = getLocalOwner;
    this.getEnemyOwner = getEnemyOwner;
    this.coord = coord;
    this.log = log;
    this.pause = pause;
  }

  /** Resolves each persistent route to only the waypoint allowed this turn. */
  turnOrders(moveOrders) {
    return moveOrders
      .map((order) => {
        const actor = this.getUnit(order.unitId);
        if (!actor) return null;
        if (UNIT_TYPES[actor.type]?.airUnit || !Array.isArray(order.waypoints))
          return order;
        const waypointIndex = Number.isInteger(order.waypointIndex)
          ? order.waypointIndex
          : 0;
        const waypoint = order.waypoints[waypointIndex];
        return waypoint
          ? { ...order, x: waypoint.x, y: waypoint.y }
          : null;
      })
      .filter(Boolean);
  }

  /**
   * Reserves this turn's supply for persistent routes. The first waypoint was
   * paid for when the route was queued; every later waypoint is paid
   * independently, so a long journey never consumes all future turns at once.
   */
  fundedOrdersForTurn(moveOrders, players) {
    const availableByOwner = new Map(
      players.map((player) => [player.id, player.resources]),
    );
    return moveOrders.filter((order) => {
      const actor = this.getUnit(order.unitId);
      if (!actor || !this.routeSegmentNeedsPayment(order, actor)) return true;
      const cost = this.routeSegmentCost(order);
      const available = availableByOwner.get(order.owner) ?? 0;
      if (available < cost) {
        if (actor.owner === this.getLocalOwner())
          this.log(
            `${UNIT_TYPES[actor.type].label} route held at ${this.coord(actor.x, actor.y)}: ◆ ${cost} required for this turn's movement.`,
            true,
          );
        return false;
      }
      availableByOwner.set(order.owner, available - cost);
      return true;
    });
  }

  /** Charges only later route segments that actually moved this turn. */
  chargeSuccessfulRouteSegments(moveOrders, successfulMoves, players) {
    const orderById = new Map(moveOrders.map((order) => [order.id, order]));
    const playerByOwner = new Map(
      players.map((player) => [player.id, player]),
    );
    for (const successfulMove of successfulMoves) {
      const order = orderById.get(successfulMove.id);
      const actor = order ? this.getUnit(order.unitId) : null;
      if (!order || !actor || !this.routeSegmentNeedsPayment(order, actor))
        continue;
      const player = playerByOwner.get(order.owner);
      if (!player) continue;
      const cost = this.routeSegmentCost(order);
      player.resources = Math.max(0, player.resources - cost);
      order.paidWaypointIndex = order.waypointIndex;
    }
  }

  routeSegmentCost(order) {
    const cost = Number(order.segmentCost ?? order.cost);
    return Number.isFinite(cost) && cost > 0 ? cost : 0;
  }

  routeSegmentNeedsPayment(order, actor = this.getUnit(order.unitId)) {
    if (
      !actor ||
      UNIT_TYPES[actor.type]?.airUnit ||
      !Array.isArray(order.waypoints)
    )
      return false;
    const waypointIndex = Number.isInteger(order.waypointIndex)
      ? order.waypointIndex
      : 0;
    // Waypoint zero keeps the existing queue-time prepayment and refund rules.
    return waypointIndex > 0 && order.paidWaypointIndex !== waypointIndex;
  }

  /** Rejects occupied destinations and collisions between queued movers. */
  successfulOrders(moveOrders) {
    const destinations = new Map();
    for (const order of moveOrders) {
      const actor = this.getUnit(order.unitId);
      if (actor && UNIT_TYPES[actor.type]?.airUnit) continue;
      const key = `${order.x},${order.y}`;
      destinations.set(key, (destinations.get(key) || 0) + 1);
    }
    return moveOrders.filter((order) => {
      const actor = this.getUnit(order.unitId);
      if (!actor) return false;
      if (UNIT_TYPES[actor.type]?.airUnit) return true;
      const key = `${order.x},${order.y}`;
      return (
        destinations.get(key) === 1 &&
        !this.isGroundOccupied(order.x, order.y, false)
      );
    });
  }

  async animate(moveOrders) {
    const battlefield = this.getBattlefield();
    const localOwner = this.getLocalOwner();
    if (battlefield) {
      const moves = moveOrders
        .map((order) => {
          const actor = this.getUnit(order.unitId);
          return actor
            ? {
                id: actor.id,
                actorType: actor.type,
                owner: actor.owner === localOwner ? 0 : 1,
                from: { x: actor.x, y: actor.y },
                to: { x: order.x, y: order.y },
              }
            : null;
        })
        .filter(Boolean);
      await battlefield.animateMoves(moves);
      return;
    }

    const animations = moveOrders.map((order) => {
      const actor = this.getUnit(order.unitId);
      if (!actor) return Promise.resolve();
      const sourceVisible = this.isVisible(actor.x, actor.y);
      const destinationVisible = this.isVisible(order.x, order.y);
      if (
        actor.owner === this.getEnemyOwner() &&
        !sourceVisible &&
        !destinationVisible
      )
        return Promise.resolve();

      const sourceTile = document.querySelector(
        `[data-testid="tile-${this.coord(actor.x, actor.y)}"]`,
      );
      const destinationTile = document.querySelector(
        `[data-testid="tile-${this.coord(order.x, order.y)}"]`,
      );
      const original = sourceTile?.querySelector(".piece");
      if (!sourceTile || !destinationTile || !original)
        return Promise.resolve();

      const sourceRect = original.getBoundingClientRect();
      const destinationRect = destinationTile.getBoundingClientRect();
      const ghost = original.cloneNode(true);
      ghost.classList.add("movement-ghost");
      ghost.style.left = `${sourceRect.left}px`;
      ghost.style.top = `${sourceRect.top}px`;
      ghost.style.width = `${sourceRect.width}px`;
      ghost.style.height = `${sourceRect.height}px`;
      document.body.appendChild(ghost);
      original.style.opacity = "0";

      const destinationCenterX =
        destinationRect.left + destinationRect.width / 2;
      const destinationCenterY =
        destinationRect.top + destinationRect.height / 2;
      const sourceCenterX = sourceRect.left + sourceRect.width / 2;
      const sourceCenterY = sourceRect.top + sourceRect.height / 2;
      const movementDistance = Math.hypot(
        order.x - actor.x,
        order.y - actor.y,
      );
      const movementDuration = UNIT_TYPES[actor.type]?.airUnit
        ? Math.min(4200, Math.max(1600, 1200 + movementDistance * 70))
        : 720;
      const animation = ghost.animate(
        [
          { transform: "translate(0, 0) scale(1)", opacity: 1 },
          {
            transform: `translate(${(destinationCenterX - sourceCenterX) * 0.55}px, ${(destinationCenterY - sourceCenterY) * 0.55 - 6}px) scale(1.08)`,
            opacity: 1,
            offset: 0.55,
          },
          {
            transform: `translate(${destinationCenterX - sourceCenterX}px, ${destinationCenterY - sourceCenterY}px) scale(1)`,
            opacity: 0.95,
          },
        ],
        {
          duration: movementDuration,
          easing: "cubic-bezier(.22,.75,.2,1)",
          fill: "forwards",
        },
      );
      return Promise.race([
        animation.finished.catch(() => {}),
        this.pause(movementDuration + 180),
      ]).finally(() => ghost.remove());
    });
    await Promise.all(animations);
  }

  /** Applies successful movement only after every animation has completed. */
  apply(moveOrders, successfulMoves) {
    const successfulIds = new Set(successfulMoves.map((item) => item.id));
    const localOwner = this.getLocalOwner();
    for (const order of moveOrders) {
      const actor = this.getUnit(order.unitId);
      if (!actor) continue;
      if (!successfulIds.has(order.id)) {
        if (actor.owner === localOwner)
          this.log(
            `${UNIT_TYPES[actor.type].label} movement blocked at ${this.coord(order.x, order.y)}.`,
            true,
          );
        continue;
      }
      const isAirUnit = UNIT_TYPES[actor.type]?.airUnit === true;
      const origin = { x: actor.x, y: actor.y };
      actor.x = order.x;
      actor.y = order.y;
      actor.movedThisRound = true;
      if (isAirUnit) {
        actor.airborne = true;
        actor.sortie = {
          from: origin,
          target: { x: order.x, y: order.y },
        };
      }
      if (actor.owner === localOwner)
        this.log(
          isAirUnit
            ? `${UNIT_TYPES[actor.type].label} took off for a bombing run over ${this.coord(order.x, order.y)}.`
            : `${UNIT_TYPES[actor.type].label} unit advanced to ${this.coord(order.x, order.y)}.`,
        );
    }
  }

  /** Advances successful ground routes and identifies orders that are done. */
  advanceRoutes(moveOrders, successfulMoves) {
    const successfulIds = new Set(successfulMoves.map((item) => item.id));
    const completedIds = new Set();
    for (const order of moveOrders) {
      const actor = this.getUnit(order.unitId);
      if (!actor) {
        completedIds.add(order.id);
        continue;
      }
      if (
        UNIT_TYPES[actor.type]?.airUnit ||
        !Array.isArray(order.waypoints)
      ) {
        completedIds.add(order.id);
        continue;
      }
      const waypointIndex = Number.isInteger(order.waypointIndex)
        ? order.waypointIndex
        : 0;
      if (waypointIndex >= order.waypoints.length) {
        completedIds.add(order.id);
        continue;
      }
      if (!successfulIds.has(order.id)) continue;
      order.waypointIndex = waypointIndex + 1;
      if (order.waypointIndex >= order.waypoints.length)
        completedIds.add(order.id);
    }
    return completedIds;
  }
}
