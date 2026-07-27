import { createEntityIndex } from "./entity-index.js";

export class CommandValidationError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

const isRecord = (value) =>
  value !== null && typeof value === "object" && !Array.isArray(value);

const planCost = (items) =>
  items.reduce((total, item) => total + (Number(item.cost) || 0), 0);

/**
 * Validates and prices one player's complete plan.
 *
 * The client may submit convenience fields such as paths and footprints, but
 * it may not choose ownership, prices, resources, phases, or entity identity.
 */
export function validatePlayerPlan({
  state,
  playerIndex,
  jobs,
  orders,
  unitTypes,
  buildings,
  cols,
  rows,
}) {
  if (!Array.isArray(jobs) || !Array.isArray(orders))
    throw new CommandValidationError(
      "INVALID_PLAN",
      "Jobs and orders must be arrays.",
    );
  if (jobs.length > 2000 || orders.length > 10000)
    throw new CommandValidationError("INVALID_PLAN", "The plan is too large.");

  const index = createEntityIndex(state);
  const previousJobs = state.jobs.filter((item) => item.owner === playerIndex);
  const previousOrders = state.orders.filter(
    (item) => item.owner === playerIndex,
  );
  const previousJobById = new Map(previousJobs.map((item) => [item.id, item]));
  const previousOrderById = new Map(
    previousOrders.map((item) => [item.id, item]),
  );
  const seenIds = new Set();
  const markId = (item) => {
    if (
      typeof item.id !== "string" ||
      item.id.length > 128 ||
      seenIds.has(item.id)
    )
      throw new CommandValidationError(
        "INVALID_PLAN",
        "Every command needs a unique valid id.",
      );
    seenIds.add(item.id);
  };
  const assertCoordinate = (item) => {
    if (
      !Number.isInteger(item.x) ||
      !Number.isInteger(item.y) ||
      item.x < 0 ||
      item.y < 0 ||
      item.x >= cols ||
      item.y >= rows
    )
      throw new CommandValidationError(
        "INVALID_PLAN",
        "A command target is outside the battlefield.",
      );
  };

  const sanitizedJobs = jobs.map((source) => {
    if (!isRecord(source))
      throw new CommandValidationError(
        "INVALID_PLAN",
        "A production job is invalid.",
      );
    markId(source);
    const previous = previousJobById.get(source.id);
    if (
      state.phase !== "deployment" &&
      (!previous ||
        previous.kind !== source.kind ||
        previous.type !== source.type)
    )
      throw new CommandValidationError(
        "INVALID_PHASE",
        "Production is only available during deployment.",
      );
    if (source.kind === "building") {
      const spec = buildings[source.type];
      if (!spec) throw new CommandValidationError("INVALID_PLAN", "Unknown building.");
      assertCoordinate(source);
      const cost = Number.isFinite(spec.price)
        ? spec.price
        : Math.max(1, Math.round(Number(source.cost)));
      if (!Number.isFinite(cost) || cost < 1 || cost > 1e6)
        throw new CommandValidationError(
          "INVALID_PLAN",
          "The building cost is invalid.",
        );
      return { ...structuredClone(source), owner: playerIndex, cost };
    }
    if (source.kind === "recruit") {
      const spec = unitTypes[source.type];
      const producer = index.structure(source.producerId ?? source.barracksId);
      if (
        !spec ||
        spec.recruitable === false ||
        !producer ||
        producer.owner !== playerIndex
      )
        throw new CommandValidationError(
          "INVALID_PLAN",
          "The recruitment order has no valid producer.",
        );
      const requiredProducer = spec.productionBuilding ?? "barracks";
      if (producer.type !== requiredProducer)
        throw new CommandValidationError(
          "INVALID_PLAN",
          "That producer cannot recruit this unit.",
        );
      return {
        ...structuredClone(source),
        owner: playerIndex,
        producerId: producer.id,
        cost: spec.price,
      };
    }
    throw new CommandValidationError("INVALID_PLAN", "Unknown production job.");
  });

  const orderCounts = new Map();
  const sanitizedOrders = orders.map((source) => {
    if (!isRecord(source))
      throw new CommandValidationError("INVALID_PLAN", "An order is invalid.");
    markId(source);
    const actor = index.unit(source.unitId);
    const spec = actor ? unitTypes[actor.type] : null;
    if (!actor || !spec || actor.owner !== playerIndex || actor.hp <= 0)
      throw new CommandValidationError(
        "INVALID_PLAN",
        "An order references an unavailable unit.",
      );
    if (!["move", "attack"].includes(source.kind))
      throw new CommandValidationError("INVALID_PLAN", "Unknown order type.");
    const previous = previousOrderById.get(source.id);
    if (
      (source.kind === "move" && state.phase !== "orders") ||
      (source.kind === "attack" && state.phase !== "attack")
    ) {
      if (
        !previous ||
        previous.kind !== source.kind ||
        previous.unitId !== source.unitId
      )
      throw new CommandValidationError(
        "INVALID_PHASE",
        "That order is not available in this phase.",
      );
    }
    assertCoordinate(source);
    if (source.kind === "attack") {
      if ((spec.damage ?? 0) <= 0 || (spec.range ?? 0) <= 0)
        throw new CommandValidationError(
          "INVALID_PLAN",
          "This unit cannot attack.",
        );
      if (source.targetId) {
        const target = index.get(source.targetId);
        if (
          !target ||
          target.owner === playerIndex ||
          target.hp <= 0 ||
          buildings[target.type]?.capturable === true
        )
          throw new CommandValidationError(
            "INVALID_PLAN",
            "The attack target is invalid.",
          );
      }
    }
    const key = `${actor.id}:${source.kind}`;
    const nextCount = (orderCounts.get(key) ?? 0) + 1;
    const maximum =
      source.kind === "attack" && Number.isInteger(spec.totalAttackActions)
        ? Math.max(0, spec.totalAttackActions - (actor.attackActionsUsed ?? 0))
        : 1;
    if (nextCount > maximum)
      throw new CommandValidationError(
        "INVALID_PLAN",
        "A unit received too many orders.",
      );
    orderCounts.set(key, nextCount);
    return {
      ...structuredClone(source),
      owner: playerIndex,
      unitId: actor.id,
      cost: spec.actionCost,
    };
  });

  const currentResources = Number(state.players[playerIndex]?.resources);
  const resources =
    currentResources +
    planCost(previousJobs) +
    planCost(previousOrders) -
    planCost(sanitizedJobs) -
    planCost(sanitizedOrders);
  if (!Number.isFinite(resources) || resources < 0)
    throw new CommandValidationError(
      "INSUFFICIENT_RESOURCES",
      "The submitted plan costs more supply than is available.",
    );

  return { jobs: sanitizedJobs, orders: sanitizedOrders, resources };
}
