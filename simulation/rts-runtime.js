const DEFAULT_TICK_RATE = 20;
const EPSILON = 1e-6;

function distanceBetween(first, second) {
  return Math.hypot(first.x - second.x, first.y - second.y);
}

function positionOf(entity) {
  return {
    x: Number.isFinite(entity.worldX) ? entity.worldX : entity.x,
    y: Number.isFinite(entity.worldY) ? entity.worldY : entity.y,
  };
}

function hashSeed(value) {
  const text = String(value ?? "rule-of-war-rts");
  let hash = 2166136261;
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

function createRandom(seed) {
  let value = hashSeed(seed);
  return () => {
    value += 0x6d2b79f5;
    let mixed = value;
    mixed = Math.imul(mixed ^ (mixed >>> 15), mixed | 1);
    mixed ^= mixed + Math.imul(mixed ^ (mixed >>> 7), mixed | 61);
    return ((mixed ^ (mixed >>> 14)) >>> 0) / 4294967296;
  };
}

function normalizeRealtimeUnit(unit) {
  unit.worldX = Number.isFinite(unit.worldX) ? unit.worldX : unit.x;
  unit.worldY = Number.isFinite(unit.worldY) ? unit.worldY : unit.y;
  unit.previousWorldX = Number.isFinite(unit.previousWorldX)
    ? unit.previousWorldX
    : unit.worldX;
  unit.previousWorldY = Number.isFinite(unit.previousWorldY)
    ? unit.previousWorldY
    : unit.worldY;
  unit.velocityX = Number(unit.velocityX) || 0;
  unit.velocityY = Number(unit.velocityY) || 0;
  unit.heading = Number(unit.heading) || (unit.owner === 0 ? 0 : Math.PI);
  unit.stance =
    unit.stance === "aggressive" ? "attack" : unit.stance || "attack";
  unit.weaponCooldownTicks = Number(unit.weaponCooldownTicks) || 0;
  unit.weaponWindupTicks = Number(unit.weaponWindupTicks) || 0;
  unit.stationaryTicks = Number(unit.stationaryTicks) || 0;
  unit.firingLocked = Boolean(unit.firingLocked);
  unit.acquiredTargetId = unit.acquiredTargetId ?? null;
  unit.explicitTargetId = unit.explicitTargetId ?? null;
  unit.path = Array.isArray(unit.path) ? unit.path : [];
  unit.pathIndex = Number.isInteger(unit.pathIndex) ? unit.pathIndex : 0;
  unit.stuckTicks = Number(unit.stuckTicks) || 0;
  delete unit.movedThisRound;
  delete unit.attackActionsUsed;
  return unit;
}

export function rtsStatsFor(spec = {}, tickRate = DEFAULT_TICK_RATE) {
  const speed = Number.isFinite(spec.speed)
    ? spec.speed
    : Math.max(0.45, Number(spec.movementRange ?? 8) / 5);
  const reloadSeconds = Number.isFinite(spec.reloadSeconds)
    ? spec.reloadSeconds
    : Math.max(0.7, 2.8 - Number(spec.hitChance ?? 0.5));
  return {
    speed,
    acceleration: Number.isFinite(spec.acceleration)
      ? spec.acceleration
      : speed * 4,
    radius: Number.isFinite(spec.collisionRadius)
      ? spec.collisionRadius
      : spec.airUnit
        ? 0.22
        : 0.34,
    acquisitionRange: Number.isFinite(spec.acquisitionRange)
      ? spec.acquisitionRange
      : Number(spec.range ?? 0),
    weaponRange: Number(spec.range ?? 0),
    reloadTicks: Math.max(1, Math.round(reloadSeconds * tickRate)),
    windupTicks: Math.max(
      0,
      Math.round(Number(spec.windupSeconds ?? 0.16) * tickRate),
    ),
    projectileSpeed: Number(spec.projectileSpeed ?? 0),
    damageRadius: Math.max(0.05, Number(spec.damageRadius ?? 0.2)),
  };
}

export function initializeRealtimeState(
  state,
  { tickRate = DEFAULT_TICK_RATE } = {},
) {
  state.mode = "realtime";
  state.phase = "realtime";
  state.resolving = false;
  state.resolutionStep = null;
  state.simulationTick = Number.isInteger(state.simulationTick)
    ? state.simulationTick
    : 0;
  state.tickRate = tickRate;
  state.elapsedSeconds = state.simulationTick / tickRate;
  state.projectiles = Array.isArray(state.projectiles) ? state.projectiles : [];
  state.nextRuntimeId = Number.isInteger(state.nextRuntimeId)
    ? state.nextRuntimeId
    : 1;
  for (const unit of state.units) normalizeRealtimeUnit(unit);
  return state;
}

export class SpatialHash {
  constructor(cellSize = 8) {
    this.cellSize = Math.max(1, cellSize);
    this.cells = new Map();
  }

  key(x, y) {
    return `${Math.floor(x / this.cellSize)},${Math.floor(y / this.cellSize)}`;
  }

  rebuild(entities) {
    this.cells.clear();
    for (const entity of entities) {
      if (entity.hp <= 0) continue;
      const position = positionOf(entity);
      const key = this.key(position.x, position.y);
      if (!this.cells.has(key)) this.cells.set(key, []);
      this.cells.get(key).push(entity);
    }
  }

  query(position, radius) {
    const minimumX = Math.floor((position.x - radius) / this.cellSize);
    const maximumX = Math.floor((position.x + radius) / this.cellSize);
    const minimumY = Math.floor((position.y - radius) / this.cellSize);
    const maximumY = Math.floor((position.y + radius) / this.cellSize);
    const result = [];
    for (let y = minimumY; y <= maximumY; y += 1)
      for (let x = minimumX; x <= maximumX; x += 1)
        result.push(...(this.cells.get(`${x},${y}`) ?? []));
    return result;
  }
}

export class RtsRuntime {
  constructor({
    state,
    unitTypes,
    buildingTypes = {},
    tickRate = DEFAULT_TICK_RATE,
    seed = "rule-of-war-rts",
    createId = null,
    findPath = null,
    isPassable = () => true,
    isVisible = () => true,
    onEvent = null,
  }) {
    this.state = initializeRealtimeState(state, { tickRate });
    this.unitTypes = unitTypes;
    this.buildingTypes = buildingTypes;
    this.tickRate = tickRate;
    this.dt = 1 / tickRate;
    this.random = createRandom(seed);
    this.createId =
      createId ??
      ((prefix) => `${prefix}-rt-${this.state.nextRuntimeId++}`);
    this.findPath = findPath;
    this.isPassable = isPassable;
    this.isVisible = isVisible;
    this.onEvent = onEvent;
    this.events = [];
    this.unitIndex = new Map();
    this.entityIndex = new Map();
    this.spatial = new SpatialHash(8);
    this.commandSequence = 0;
    this.aiNextTick = 0;
    this.combatOwners = new Set(
      [...state.units, ...state.structures]
        .filter(
          (entity) =>
            entity.owner >= 0 &&
            (this.unitTypes[entity.type] || entity.type === "barracks"),
        )
        .map((entity) => entity.owner),
    );
    for (const unit of this.state.units) this.normalizeAircraft(unit);
    this.rebuildIndexes();
  }

  normalizeAircraft(unit) {
    const spec = this.unitTypes[unit.type];
    if (!spec?.airUnit) return unit;
    const capacity = Math.max(1, Number(spec.fuelCapacity ?? spec.movementRange));
    unit.maxFuel = Math.max(1, Number(unit.maxFuel ?? capacity));
    unit.fuel = Math.max(
      0,
      Math.min(unit.maxFuel, Number(unit.fuel ?? unit.maxFuel)),
    );
    unit.airborne = Boolean(unit.airborne);
    unit.airState =
      unit.airState ??
      (unit.airborne
        ? "active"
        : unit.fuel < unit.maxFuel
          ? "refueling"
          : "landed");
    unit.fuelCrashEmitted = Boolean(unit.fuelCrashEmitted);
    return unit;
  }

  emit(type, payload = {}) {
    const event = {
      id: this.createId("event"),
      tick: this.state.simulationTick,
      type,
      ...payload,
    };
    this.events.push(event);
    this.onEvent?.(event);
    return event;
  }

  drainEvents() {
    return this.events.splice(0);
  }

  rebuildIndexes() {
    for (const unit of this.state.units) normalizeRealtimeUnit(unit);
    this.unitIndex = new Map(this.state.units.map((unit) => [unit.id, unit]));
    this.entityIndex = new Map(
      [...this.state.units, ...this.state.structures].map((entity) => [
        entity.id,
        entity,
      ]),
    );
    this.spatial.rebuild([...this.state.units, ...this.state.structures]);
  }

  issueCommand({
    owner,
    unitIds,
    type,
    destination = null,
    targetId = null,
    queued = false,
  }) {
    const allowedTypes = new Set([
      "move",
      "attack",
      "attack-move",
      "stop",
      "hold",
      "patrol",
      "attack-stance",
      "flee",
      "hold-fire",
    ]);
    if (!allowedTypes.has(type) || !Array.isArray(unitIds)) return [];
    const accepted = [];
    for (const unitId of unitIds) {
      const unit = this.unitIndex.get(unitId);
      if (!unit || unit.owner !== owner || unit.hp <= 0) continue;
      const unitType = this.unitTypes[unit.type];
      const existingCommand = this.commandFor(unit.id);
      if (["attack-stance", "flee", "hold-fire"].includes(type)) {
        unit.stance =
          type === "attack-stance"
            ? "attack"
            : type === "hold-fire"
              ? "hold-fire"
              : "flee";
        unit.acquiredTargetId = null;
        unit.explicitTargetId = null;
        if (type !== "attack-stance") {
          this.clearCommands(unit.id);
          unit.path = [];
          unit.pathIndex = 0;
          unit.velocityX = 0;
          unit.velocityY = 0;
        }
        if (type === "hold-fire") unit.weaponWindupTicks = 0;
        accepted.push(unit.id);
        continue;
      }
      if (!queued) this.clearCommands(unit.id);
      if (type === "stop" || type === "hold") {
        unit.path = [];
        unit.pathIndex = 0;
        unit.velocityX = 0;
        unit.velocityY = 0;
        unit.explicitTargetId = null;
        unit.acquiredTargetId = null;
        unit.stance = type === "hold" ? "hold-fire" : unit.stance;
        accepted.push(unit.id);
        continue;
      }
      if (type === "attack" && !this.entityIndex.has(targetId)) continue;
      if (
        type !== "attack" &&
        (!destination ||
          !Number.isFinite(destination.x) ||
          !Number.isFinite(destination.y))
      )
        continue;
      if (unitType?.airUnit) {
        this.normalizeAircraft(unit);
        if (unit.airState === "returning" || unit.airState === "refueling") {
          this.emit("aircraft-command-rejected", {
            unitId: unit.id,
            reason: unit.airState,
          });
          continue;
        }
        const commandDestination =
          type === "attack"
            ? positionOf(this.entityIndex.get(targetId))
            : destination;
        if (
          commandDestination &&
          !this.aircraftCanReachAndReturn(unit, commandDestination)
        ) {
          this.emit("aircraft-command-rejected", {
            unitId: unit.id,
            reason: "insufficient-return-fuel",
          });
          continue;
        }
        if (!unit.airborne) {
          unit.airborne = true;
          unit.airState = "active";
          this.emit("aircraft-launched", {
            unitId: unit.id,
            airfieldId: unit.homeAirfieldId ?? null,
          });
        }
      }
      const command = {
        id: this.createId("command"),
        sequence: ++this.commandSequence,
        kind: "command",
        commandType: type,
        owner,
        unitId: unit.id,
        targetId: targetId ?? null,
        x: destination?.x ?? null,
        y: destination?.y ?? null,
        issuedAtTick: this.state.simulationTick,
      };
      this.state.orders.push(command);
      if (!queued || !existingCommand) this.activateCommand(unit, command);
      accepted.push(unit.id);
    }
    if (accepted.length)
      this.emit("command-issued", { owner, unitIds: accepted, commandType: type });
    return accepted;
  }

  clearCommands(unitId) {
    this.state.orders = this.state.orders.filter(
      (order) => order.unitId !== unitId,
    );
  }

  commandFor(unitId) {
    return this.state.orders.find((order) => order.unitId === unitId) ?? null;
  }

  activateCommand(unit, command = this.commandFor(unit.id)) {
    if (!unit || !command) return false;
    if (command.commandType === "attack") {
      unit.explicitTargetId = command.targetId;
      unit.path = [];
      unit.pathIndex = 0;
      return true;
    }
    if (Number.isFinite(command.x) && Number.isFinite(command.y)) {
      this.planUnitPath(unit, { x: command.x, y: command.y });
      return true;
    }
    return false;
  }

  completeCommand(unit, command) {
    this.state.orders = this.state.orders.filter(
      (order) => order.id !== command?.id,
    );
    this.activateCommand(unit);
  }

  planUnitPath(unit, destination) {
    const start = positionOf(unit);
    const rawPath =
      this.unitTypes[unit.type]?.airUnit
        ? [start, destination]
        : (this.findPath?.(unit, start, destination) ?? [start, destination]);
    const path = (Array.isArray(rawPath) ? rawPath : rawPath?.path ?? [])
      .map((point) => ({
        x: Number.isFinite(point.worldX) ? point.worldX : point.x,
        y: Number.isFinite(point.worldY) ? point.worldY : point.y,
      }))
      .filter(
        (point) => Number.isFinite(point.x) && Number.isFinite(point.y),
      );
    if (path.length && distanceBetween(path[0], start) < 0.1) path.shift();
    unit.path = path;
    unit.pathIndex = 0;
    unit.pathDestination = { ...destination };
    unit.nextRepathTick = this.state.simulationTick + this.tickRate;
    return path;
  }

  step(ticks = 1) {
    for (let index = 0; index < ticks; index += 1) this.stepOnce();
    return this.state;
  }

  stepOnce() {
    if (this.state.winner) return;
    this.state.simulationTick += 1;
    this.state.elapsedSeconds = this.state.simulationTick / this.tickRate;
    this.rebuildIndexes();
    this.updateProduction();
    this.updateEconomy();
    this.updateAircraft();
    this.updateBehaviors();
    this.updateMovement();
    this.rebuildIndexes();
    this.updateTargeting();
    this.updateWeapons();
    this.updateProjectiles();
    this.updateSupport();
    this.updateObjectives();
    this.removeDestroyed();
    this.updateVictory();
  }

  updateMovement() {
    const moving = this.state.units
      .filter(
        (unit) =>
          unit.hp > 0 &&
          Array.isArray(unit.path) &&
          unit.pathIndex < unit.path.length,
      )
      .sort((first, second) => String(first.id).localeCompare(String(second.id)));
    for (const unit of moving) {
      const type = this.unitTypes[unit.type];
      const spec = rtsStatsFor(type, this.tickRate);
      const current = positionOf(unit);
      unit.previousWorldX = current.x;
      unit.previousWorldY = current.y;
      const waypoint = unit.path[unit.pathIndex];
      if (type?.requiresStationaryToFire && unit.firingLocked) {
        unit.velocityX = 0;
        unit.velocityY = 0;
        continue;
      }
      const dx = waypoint.x - current.x;
      const dy = waypoint.y - current.y;
      const remaining = Math.hypot(dx, dy);
      if (remaining < 0.08) {
        unit.pathIndex += 1;
        if (unit.pathIndex >= unit.path.length) this.completeMovement(unit);
        continue;
      }
      let desiredX = dx / remaining;
      let desiredY = dy / remaining;
      if (!this.unitTypes[unit.type]?.airUnit) {
        const neighbors = this.spatial.query(current, spec.radius * 4);
        for (const neighbor of neighbors) {
          if (neighbor.id === unit.id || !this.unitIndex.has(neighbor.id))
            continue;
          const neighborPosition = positionOf(neighbor);
          const separation = distanceBetween(current, neighborPosition);
          const minimum = spec.radius + rtsStatsFor(
            this.unitTypes[neighbor.type],
            this.tickRate,
          ).radius;
          if (separation > EPSILON && separation < minimum * 1.8) {
            const force =
              ((minimum * 1.8 - separation) / (minimum * 1.8)) * 0.35;
            desiredX += ((current.x - neighborPosition.x) / separation) * force;
            desiredY += ((current.y - neighborPosition.y) / separation) * force;
          }
        }
      }
      const desiredLength = Math.hypot(desiredX, desiredY) || 1;
      const brakingSpeed = Math.sqrt(
        Math.max(0, 2 * spec.acceleration * remaining),
      );
      const desiredSpeed = Math.min(spec.speed, brakingSpeed);
      desiredX = (desiredX / desiredLength) * desiredSpeed;
      desiredY = (desiredY / desiredLength) * desiredSpeed;
      const maximumVelocityChange = spec.acceleration * this.dt;
      unit.velocityX += Math.max(
        -maximumVelocityChange,
        Math.min(maximumVelocityChange, desiredX - unit.velocityX),
      );
      unit.velocityY += Math.max(
        -maximumVelocityChange,
        Math.min(maximumVelocityChange, desiredY - unit.velocityY),
      );
      let stepX = unit.velocityX * this.dt;
      let stepY = unit.velocityY * this.dt;
      const stepLength = Math.hypot(stepX, stepY);
      if (stepLength > remaining && stepLength > EPSILON) {
        stepX = (stepX / stepLength) * remaining;
        stepY = (stepY / stepLength) * remaining;
      }
      const next = {
        x: current.x + stepX,
        y: current.y + stepY,
      };
      if (
        this.unitTypes[unit.type]?.airUnit ||
        this.isPassable(next.x, next.y, unit)
      ) {
        unit.worldX = next.x;
        unit.worldY = next.y;
        unit.x = Math.round(next.x);
        unit.y = Math.round(next.y);
        if (Math.hypot(unit.velocityX, unit.velocityY) > 0.015)
          unit.heading = Math.atan2(-unit.velocityY, unit.velocityX);
        if (type?.airUnit && unit.airborne) {
          const fuelBurn =
            Math.hypot(stepX, stepY) * Number(type.fuelBurnPerDistance ?? 1);
          unit.fuel = Math.max(0, Number(unit.fuel ?? 0) - fuelBurn);
          if (unit.fuel <= EPSILON) this.crashAircraft(unit);
        }
        unit.stuckTicks = 0;
      } else {
        unit.velocityX = 0;
        unit.velocityY = 0;
        unit.stuckTicks += 1;
        if (
          unit.stuckTicks >= this.tickRate / 2 &&
          unit.pathDestination &&
          this.state.simulationTick >= unit.nextRepathTick
        )
          this.planUnitPath(unit, unit.pathDestination);
      }
    }
  }

  friendlyAirfields(unit) {
    return this.state.structures.filter(
      (structure) =>
        structure.owner === unit.owner &&
        structure.type === "airfield" &&
        structure.hp > 0,
    );
  }

  nearestFriendlyAirfield(unit, from = positionOf(unit)) {
    return this.friendlyAirfields(unit).sort(
      (first, second) =>
        distanceBetween(from, positionOf(first)) -
          distanceBetween(from, positionOf(second)) ||
        String(first.id).localeCompare(String(second.id)),
    )[0] ?? null;
  }

  aircraftCanReachAndReturn(unit, destination) {
    const spec = this.unitTypes[unit.type];
    if (!spec?.airUnit) return true;
    const destinationAirfield = this.nearestFriendlyAirfield(unit, destination);
    if (!destinationAirfield) return false;
    const burnRate = Number(spec.fuelBurnPerDistance ?? 1);
    const reserve = Number(spec.returnFuelReserve ?? 0);
    const requiredFuel =
      (distanceBetween(positionOf(unit), destination) +
        distanceBetween(destination, positionOf(destinationAirfield))) *
        burnRate +
      reserve;
    return Number(unit.fuel ?? 0) + EPSILON >= requiredFuel;
  }

  updateAircraft() {
    for (const unit of this.state.units) {
      const spec = this.unitTypes[unit.type];
      if (!spec?.airUnit || unit.hp <= 0) continue;
      this.normalizeAircraft(unit);
      if (!unit.airborne) {
        if (unit.airState !== "refueling") continue;
        const refuelPerTick =
          unit.maxFuel /
          Math.max(1, Number(spec.refuelSeconds ?? 8) * this.tickRate);
        unit.fuel = Math.min(unit.maxFuel, unit.fuel + refuelPerTick);
        if (unit.fuel + EPSILON >= unit.maxFuel) {
          unit.fuel = unit.maxFuel;
          unit.airState = "landed";
          this.emit("aircraft-refuelled", {
            unitId: unit.id,
            airfieldId: unit.homeAirfieldId ?? null,
          });
        }
        continue;
      }

      if (unit.fuel <= EPSILON) {
        this.crashAircraft(unit);
        continue;
      }
      const current = positionOf(unit);
      const nearest = this.nearestFriendlyAirfield(unit, current);
      if (!nearest) continue;
      const distanceHome = distanceBetween(current, positionOf(nearest));
      const requiredFuel =
        distanceHome * Number(spec.fuelBurnPerDistance ?? 1) +
        Number(spec.returnFuelReserve ?? 0);
      if (
        unit.airState !== "returning" &&
        Number(unit.fuel) <= requiredFuel + EPSILON
      )
        this.returnAircraftToBase(unit, nearest);
      if (
        unit.airState === "returning" &&
        distanceHome <= Math.max(0.35, rtsStatsFor(spec, this.tickRate).speed * this.dt)
      )
        this.landAircraft(unit, nearest);
    }
  }

  returnAircraftToBase(unit, airfield) {
    this.clearCommands(unit.id);
    unit.explicitTargetId = null;
    unit.acquiredTargetId = null;
    unit.weaponWindupTicks = 0;
    unit.airState = "returning";
    unit.homeAirfieldId = airfield.id;
    this.planUnitPath(unit, positionOf(airfield));
    this.emit("aircraft-returning", {
      unitId: unit.id,
      airfieldId: airfield.id,
      fuel: unit.fuel,
    });
  }

  landAircraft(unit, airfield) {
    this.clearCommands(unit.id);
    unit.path = [];
    unit.pathIndex = 0;
    unit.velocityX = 0;
    unit.velocityY = 0;
    unit.worldX = airfield.x;
    unit.worldY = airfield.y;
    unit.x = airfield.x;
    unit.y = airfield.y;
    unit.airborne = false;
    unit.airState = "refueling";
    unit.homeAirfieldId = airfield.id;
    this.emit("aircraft-landed", {
      unitId: unit.id,
      airfieldId: airfield.id,
      fuel: unit.fuel,
    });
  }

  crashAircraft(unit) {
    if (unit.fuelCrashEmitted) return;
    unit.fuelCrashEmitted = true;
    unit.fuel = 0;
    unit.hp = 0;
    unit.velocityX = 0;
    unit.velocityY = 0;
    unit.path = [];
    unit.pathIndex = 0;
    this.clearCommands(unit.id);
    this.emit("aircraft-crashed", {
      unitId: unit.id,
      owner: unit.owner,
      x: unit.worldX,
      y: unit.worldY,
      reason: "fuel-exhausted",
    });
  }

  updateBehaviors() {
    if (this.state.simulationTick % Math.max(1, Math.round(this.tickRate / 4)))
      return;
    for (const unit of this.state.units) {
      if (unit.hp <= 0 || unit.stance !== "flee") continue;
      const stats = rtsStatsFor(this.unitTypes[unit.type], this.tickRate);
      const current = positionOf(unit);
      const threats = this.spatial
        .query(current, stats.acquisitionRange)
        .filter(
          (entity) =>
            entity.owner >= 0 &&
            entity.owner !== unit.owner &&
            entity.hp > 0 &&
            distanceBetween(current, positionOf(entity)) <=
              stats.acquisitionRange,
        );
      if (!threats.length) continue;
      let awayX = 0;
      let awayY = 0;
      for (const threat of threats) {
        const threatPosition = positionOf(threat);
        const dx = current.x - threatPosition.x;
        const dy = current.y - threatPosition.y;
        const distance = Math.max(0.2, Math.hypot(dx, dy));
        awayX += dx / (distance * distance);
        awayY += dy / (distance * distance);
      }
      const length = Math.hypot(awayX, awayY);
      if (length < EPSILON) continue;
      const retreatDistance = Math.max(4, stats.weaponRange * 0.45);
      const destination = {
        x: current.x + (awayX / length) * retreatDistance,
        y: current.y + (awayY / length) * retreatDistance,
      };
      if (!this.isPassable(destination.x, destination.y, unit)) continue;
      if (
        !unit.pathDestination ||
        distanceBetween(unit.pathDestination, destination) > 1.5
      )
        this.planUnitPath(unit, destination);
      unit.acquiredTargetId = null;
      unit.explicitTargetId = null;
      unit.weaponWindupTicks = 0;
    }
  }

  completeMovement(unit) {
    unit.velocityX = 0;
    unit.velocityY = 0;
    unit.path = [];
    unit.pathIndex = 0;
    const command = this.commandFor(unit.id);
    if (command?.commandType === "patrol") {
      const destination = { x: command.x, y: command.y };
      command.x = unit.patrolOriginX ?? unit.worldX;
      command.y = unit.patrolOriginY ?? unit.worldY;
      unit.patrolOriginX ??= unit.previousWorldX;
      unit.patrolOriginY ??= unit.previousWorldY;
      this.planUnitPath(unit, destination);
      return;
    }
    if (command?.commandType === "move" || command?.commandType === "attack-move")
      this.completeCommand(unit, command);
    this.emit("unit-arrived", { unitId: unit.id, position: positionOf(unit) });
  }

  targetCandidates(unit, stats) {
    const position = positionOf(unit);
    return this.spatial
      .query(position, stats.acquisitionRange)
      .filter(
        (candidate) =>
          candidate.owner >= 0 &&
          candidate.owner !== unit.owner &&
          candidate.hp > 0 &&
          distanceBetween(position, positionOf(candidate)) <=
            stats.acquisitionRange &&
          this.isVisible(unit.owner, candidate),
      );
  }

  updateTargeting() {
    for (const unit of this.state.units) {
      const type = this.unitTypes[unit.type];
      if (
        !type ||
        type.damage <= 0 ||
        (type.airUnit && unit.airState !== "active") ||
        unit.stance === "hold-fire" ||
        unit.stance === "flee"
      )
        continue;
      const stats = rtsStatsFor(type, this.tickRate);
      const explicit = this.entityIndex.get(unit.explicitTargetId);
      if (explicit && explicit.hp > 0 && explicit.owner !== unit.owner) {
        unit.acquiredTargetId = explicit.id;
        continue;
      }
      unit.explicitTargetId = null;
      const current = this.entityIndex.get(unit.acquiredTargetId);
      if (
        current &&
        current.hp > 0 &&
        distanceBetween(positionOf(unit), positionOf(current)) <=
          stats.acquisitionRange * 1.15 &&
        this.isVisible(unit.owner, current)
      )
        continue;
      const candidates = this.targetCandidates(unit, stats);
      candidates.sort((first, second) => {
        const firstDistance = distanceBetween(positionOf(unit), positionOf(first));
        const secondDistance = distanceBetween(
          positionOf(unit),
          positionOf(second),
        );
        const firstScore = firstDistance + first.hp * 0.15;
        const secondScore = secondDistance + second.hp * 0.15;
        return firstScore - secondScore ||
          String(first.id).localeCompare(String(second.id));
      });
      unit.acquiredTargetId = candidates[0]?.id ?? null;
    }
  }

  updateWeapons() {
    for (const unit of this.state.units) {
      const type = this.unitTypes[unit.type];
      if (unit.weaponCooldownTicks > 0) unit.weaponCooldownTicks -= 1;
      if (unit.weaponWindupTicks > 0) {
        unit.weaponWindupTicks -= 1;
        if (unit.weaponWindupTicks === 0) this.fireWeapon(unit);
        continue;
      }
      const target = this.entityIndex.get(unit.acquiredTargetId);
      if (!target || target.hp <= 0) {
        unit.firingLocked = false;
        unit.stationaryTicks = 0;
        continue;
      }
      const stats = rtsStatsFor(type, this.tickRate);
      const range = distanceBetween(positionOf(unit), positionOf(target));
      if (range > stats.weaponRange) {
        unit.firingLocked = false;
        unit.stationaryTicks = 0;
        const command = this.commandFor(unit.id);
        if (
          command?.commandType === "attack" &&
          ["attack", "aggressive"].includes(unit.stance) &&
          (!unit.path.length ||
            distanceBetween(unit.pathDestination ?? {}, positionOf(target)) > 1)
        )
          this.planUnitPath(unit, positionOf(target));
        continue;
      }
      const origin = positionOf(unit);
      const targetPosition = positionOf(target);
      unit.heading = Math.atan2(
        -(targetPosition.y - origin.y),
        targetPosition.x - origin.x,
      );
      if (type?.requiresStationaryToFire) {
        unit.firingLocked = true;
        unit.velocityX = 0;
        unit.velocityY = 0;
        unit.stationaryTicks += 1;
        const setupTicks = Math.max(
          1,
          Math.round(Number(type.setupSeconds ?? 0.5) * this.tickRate),
        );
        if (unit.stationaryTicks < setupTicks) continue;
      } else {
        unit.firingLocked = false;
      }
      const command = this.commandFor(unit.id);
      if (command?.commandType === "attack") {
        unit.path = [];
        unit.pathIndex = 0;
        unit.velocityX = 0;
        unit.velocityY = 0;
      }
      if (unit.weaponCooldownTicks > 0) continue;
      unit.velocityX *= 0.5;
      unit.velocityY *= 0.5;
      unit.weaponWindupTicks = Math.max(1, stats.windupTicks);
    }
  }

  fireWeapon(unit) {
    const target = this.entityIndex.get(unit.acquiredTargetId);
    if (!target || target.hp <= 0) return;
    const type = this.unitTypes[unit.type];
    const stats = rtsStatsFor(type, this.tickRate);
    const origin = positionOf(unit);
    const targetPosition = positionOf(target);
    if (distanceBetween(origin, targetPosition) > stats.weaponRange) return;
    unit.heading = Math.atan2(
      -(targetPosition.y - origin.y),
      targetPosition.x - origin.x,
    );
    unit.weaponCooldownTicks = stats.reloadTicks;
    const hit = this.random() < Number(type.hitChance ?? 1);
    const damage = Number(unit.damage ?? type.damage ?? 0);
    this.emit("weapon-fired", {
      actorId: unit.id,
      targetId: target.id,
      from: origin,
      to: targetPosition,
      hit,
      damage,
    });
    if (stats.projectileSpeed > 0) {
      const travelTicks = Math.max(
        1,
        Math.ceil(
          (distanceBetween(origin, targetPosition) / stats.projectileSpeed) *
            this.tickRate,
        ),
      );
      this.state.projectiles.push({
        id: this.createId("projectile"),
        owner: unit.owner,
        actorId: unit.id,
        targetId: target.id,
        impactX: targetPosition.x,
        impactY: targetPosition.y,
        impactTick: this.state.simulationTick + travelTicks,
        hit,
        damage,
        damageRadius: stats.damageRadius,
      });
    } else
      this.resolveDamageAt({
        owner: unit.owner,
        actorId: unit.id,
        targetId: target.id,
        x: targetPosition.x,
        y: targetPosition.y,
        hit,
        damage,
        damageRadius: stats.damageRadius,
      });
  }

  resolveDamageAt({
    owner,
    actorId,
    targetId,
    x,
    y,
    hit,
    damage,
    damageRadius,
  }) {
    if (!hit || damage <= 0) return [];
    const impact = { x, y };
    const damaged = this.spatial
      .query(impact, damageRadius)
      .filter(
        (entity) =>
          entity.owner >= 0 &&
          entity.owner !== owner &&
          entity.hp > 0 &&
          distanceBetween(impact, positionOf(entity)) <= damageRadius,
      )
      .sort((first, second) =>
        String(first.id).localeCompare(String(second.id)),
      );
    for (const entity of damaged) {
      entity.hp -= damage;
      this.emit("damage-applied", {
        actorId,
        targetId: entity.id,
        lockedTargetId: targetId,
        damage,
        x,
        y,
        damageRadius,
      });
    }
    return damaged.map((entity) => entity.id);
  }

  updateProjectiles() {
    const remaining = [];
    for (const projectile of this.state.projectiles) {
      if (projectile.impactTick > this.state.simulationTick) {
        remaining.push(projectile);
        continue;
      }
      const damagedEntityIds = this.resolveDamageAt({
        owner: projectile.owner,
        actorId: projectile.actorId,
        targetId: projectile.targetId,
        x: projectile.impactX,
        y: projectile.impactY,
        hit: projectile.hit,
        damage: projectile.damage,
        damageRadius: projectile.damageRadius ?? 0.2,
      });
      this.emit("projectile-impact", {
        projectileId: projectile.id,
        targetId: projectile.targetId,
        x: projectile.impactX,
        y: projectile.impactY,
        hit: projectile.hit,
        damageRadius: projectile.damageRadius ?? 0.2,
        damagedEntityIds,
      });
    }
    this.state.projectiles = remaining;
  }

  updateEconomy() {
    if (this.state.simulationTick % this.tickRate !== 0) return;
    for (const player of this.state.players) {
      const mines = this.state.structures.filter(
        (structure) =>
          structure.owner === player.id &&
          structure.type === "mine" &&
          structure.hp > 0,
      ).length;
      const incomePerMinute = Number(
        this.state.rtsIncomePerMinePerMinute ?? 30,
      );
      player.resourceAccumulator =
        Number(player.resourceAccumulator ?? 0) + mines * incomePerMinute / 60;
      const whole = Math.floor(player.resourceAccumulator);
      if (whole > 0) {
        player.resources += whole;
        player.resourceAccumulator -= whole;
      }
    }
  }

  updateProduction() {
    for (const job of this.state.jobs) {
      job.durationTicks ??= Math.max(
        this.tickRate,
        Math.round(Number(job.durationSeconds ?? 8) * this.tickRate),
      );
      job.progressTicks = Number(job.progressTicks ?? 0) + 1;
    }
    const completed = this.state.jobs.filter(
      (job) => job.progressTicks >= job.durationTicks,
    );
    this.state.jobs = this.state.jobs.filter(
      (job) => job.progressTicks < job.durationTicks,
    );
    for (const job of completed) this.emit("production-complete", { job });
  }

  updateSupport() {
    if (this.state.simulationTick % this.tickRate !== 0) return;
    const healers = [
      ...this.state.units.filter(
        (unit) => this.unitTypes[unit.type]?.medical && unit.hp > 0,
      ),
      ...this.state.structures.filter(
        (structure) =>
          this.buildingTypes[structure.type]?.medicalFacility &&
          structure.hp > 0,
      ),
    ];
    for (const healer of healers) {
      const spec =
        this.unitTypes[healer.type] ?? this.buildingTypes[healer.type];
      const candidates = this.spatial
        .query(positionOf(healer), Number(spec.supportRange ?? 0))
        .filter((unit) => {
          const unitSpec = this.unitTypes[unit.type];
          return (
            unitSpec &&
            unit.owner === healer.owner &&
            unit.hp > 0 &&
            unit.hp < unitSpec.lives &&
            distanceBetween(positionOf(healer), positionOf(unit)) <=
              Number(spec.supportRange ?? 0)
          );
        })
        .sort((first, second) => first.hp - second.hp);
      const target = candidates[0];
      if (!target) continue;
      const maximum = this.unitTypes[target.type].lives;
      target.hp = Math.min(maximum, target.hp + Number(spec.healing ?? 1));
      this.emit("unit-healed", { healerId: healer.id, targetId: target.id });
    }
  }

  updateObjectives() {
    if (this.state.simulationTick % Math.max(1, Math.round(this.tickRate / 4)))
      return;
    const objectives = this.state.structures.filter(
      (structure) => structure.type === "objective" && structure.hp > 0,
    );
    for (const objective of objectives) {
      const nearby = this.spatial
        .query(positionOf(objective), 2.75)
        .filter(
          (entity) =>
            this.unitIndex.has(entity.id) &&
            entity.hp > 0 &&
            distanceBetween(positionOf(entity), positionOf(objective)) <= 2.75,
        );
      const strengthByOwner = new Map();
      for (const unit of nearby)
        strengthByOwner.set(
          unit.owner,
          (strengthByOwner.get(unit.owner) ?? 0) +
            Math.max(1, Number(unit.strength ?? 1)),
        );
      const contenders = [...strengthByOwner.entries()].sort(
        (first, second) => second[1] - first[1],
      );
      if (!contenders.length || contenders[0][1] === contenders[1]?.[1])
        continue;
      const captureOwner = contenders[0][0];
      if (objective.captureOwner !== captureOwner) {
        objective.captureOwner = captureOwner;
        objective.captureProgress = Math.max(
          0,
          Number(objective.captureProgress ?? 0) - 1,
        );
      } else {
        const advantage =
          contenders[0][1] - Number(contenders[1]?.[1] ?? 0);
        objective.captureProgress =
          Number(objective.captureProgress ?? 0) +
          Math.min(2, 0.25 + Math.log10(advantage + 1) * 0.2);
      }
      if (objective.captureProgress < 10) continue;
      const previousOwner = objective.owner;
      objective.owner = captureOwner;
      objective.captureProgress = 0;
      this.emit("objective-captured", {
        objectiveId: objective.id,
        owner: captureOwner,
        previousOwner,
        x: objective.x,
        y: objective.y,
      });
    }
    if (!objectives.length) return;
    for (const owner of [0, 1]) {
      const controlsAll = objectives.every(
        (objective) => objective.owner === owner,
      );
      const key = String(owner);
      this.state.objectiveVictoryProgress ??= { 0: 0, 1: 0 };
      this.state.objectiveVictoryProgress[key] = controlsAll
        ? Number(this.state.objectiveVictoryProgress[key] ?? 0) + 0.25
        : 0;
      if (this.state.objectiveVictoryProgress[key] >= 30) {
        this.state.objectiveWinner = owner;
        this.emit("objective-victory", { owner });
      }
    }
  }

  removeDestroyed() {
    const destroyed = [...this.state.units, ...this.state.structures].filter(
      (entity) => entity.hp <= 0,
    );
    if (!destroyed.length) return;
    const ids = new Set(destroyed.map((entity) => entity.id));
    this.state.units = this.state.units.filter((unit) => !ids.has(unit.id));
    this.state.structures = this.state.structures.filter(
      (structure) => !ids.has(structure.id),
    );
    const interruptedUnitIds = new Set(
      this.state.orders
        .filter((order) => ids.has(order.targetId))
        .map((order) => order.unitId),
    );
    this.state.orders = this.state.orders.filter(
      (order) => !ids.has(order.unitId) && !ids.has(order.targetId),
    );
    for (const entity of destroyed)
      this.emit("entity-destroyed", {
        entityId: entity.id,
        entityType: entity.type,
        owner: entity.owner,
        x: entity.x,
        y: entity.y,
      });
    for (const unit of this.state.units) {
      if (ids.has(unit.acquiredTargetId)) unit.acquiredTargetId = null;
      if (ids.has(unit.explicitTargetId)) unit.explicitTargetId = null;
      if (interruptedUnitIds.has(unit.id)) this.activateCommand(unit);
    }
    this.rebuildIndexes();
  }

  updateVictory() {
    if (this.state.objectiveWinner === 0 || this.state.objectiveWinner === 1) {
      this.state.winner = this.state.objectiveWinner;
    } else if (this.combatOwners.has(0) && this.combatOwners.has(1)) {
      const alive = [0, 1].map(
        (owner) =>
          this.state.units.some((unit) => unit.owner === owner && unit.hp > 0) ||
          this.state.structures.some(
            (structure) =>
              structure.owner === owner &&
              structure.type === "barracks" &&
              structure.hp > 0,
          ),
      );
      if (!alive[0] || !alive[1])
        this.state.winner = alive[0] ? 0 : alive[1] ? 1 : "draw";
    }
    if (this.state.winner != null && !this.state.winnerEventEmitted) {
      this.state.winnerEventEmitted = true;
      this.emit("match-ended", { winner: this.state.winner });
    }
  }
}

export const RTS_TICK_RATE = DEFAULT_TICK_RATE;
