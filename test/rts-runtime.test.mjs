import assert from "node:assert/strict";
import test from "node:test";

import {
  initializeRealtimeState,
  RtsRuntime,
} from "../simulation/rts-runtime.js";

const TYPES = {
  rifle: {
    lives: 10,
    damage: 2,
    range: 4,
    acquisitionRange: 7,
    hitChance: 1,
    speed: 2,
    acceleration: 20,
    reloadSeconds: 0.5,
    windupSeconds: 0,
  },
  medic: {
    lives: 5,
    damage: 0,
    range: 0,
    hitChance: 0,
    speed: 2,
    medical: true,
    healing: 1,
    supportRange: 3,
  },
  artillery: {
    lives: 10,
    damage: 2,
    range: 8,
    acquisitionRange: 9,
    hitChance: 1,
    speed: 0.5,
    acceleration: 2,
    reloadSeconds: 1,
    windupSeconds: 0,
    requiresStationaryToFire: true,
    setupSeconds: 0.25,
  },
  projectile: {
    lives: 10,
    damage: 2,
    range: 8,
    acquisitionRange: 9,
    hitChance: 1,
    speed: 2,
    acceleration: 20,
    reloadSeconds: 5,
    windupSeconds: 0,
    projectileSpeed: 4,
    damageRadius: 0.25,
  },
  splash: {
    lives: 10,
    damage: 2,
    range: 8,
    acquisitionRange: 9,
    hitChance: 1,
    speed: 0,
    acceleration: 0,
    reloadSeconds: 5,
    windupSeconds: 0,
    projectileSpeed: 10,
    damageRadius: 1.25,
    requiresStationaryToFire: true,
    setupSeconds: 0,
  },
  plane: {
    lives: 2,
    damage: 0,
    range: 0,
    acquisitionRange: 0,
    hitChance: 0,
    speed: 2,
    acceleration: 20,
    airUnit: true,
    fuelCapacity: 40,
    fuelBurnPerDistance: 1,
    returnFuelReserve: 4,
    refuelSeconds: 2,
  },
};

function unit(id, owner, x, y, type = "rifle") {
  return { id, owner, type, x, y, hp: TYPES[type].lives, damage: TYPES[type].damage };
}

function stateWith(units) {
  return {
    phase: "deployment",
    units,
    structures: [],
    players: [
      { id: 0, resources: 0 },
      { id: 1, resources: 0 },
    ],
    jobs: [],
    orders: [],
    winner: null,
  };
}

test("realtime initialization adds continuous state without losing tile coordinates", () => {
  const state = stateWith([unit("u1", 0, 2, 3)]);
  initializeRealtimeState(state, { tickRate: 20 });
  assert.equal(state.phase, "realtime");
  assert.equal(state.units[0].worldX, 2);
  assert.equal(state.units[0].worldY, 3);
  assert.equal(state.units[0].stance, "attack");
});

test("move commands advance continuously and finish at their destination", () => {
  const state = stateWith([unit("u1", 0, 0, 0)]);
  const runtime = new RtsRuntime({ state, unitTypes: TYPES, tickRate: 20 });
  runtime.issueCommand({
    owner: 0,
    unitIds: ["u1"],
    type: "move",
    destination: { x: 2, y: 0 },
  });
  runtime.step(20);
  assert.ok(state.units[0].worldX > 1.7);
  runtime.step(10);
  assert.ok(Math.abs(state.units[0].worldX - 2) < 0.1);
  assert.equal(state.orders.length, 0);
});

test("queued commands activate in order", () => {
  const state = stateWith([unit("u1", 0, 0, 0)]);
  const runtime = new RtsRuntime({ state, unitTypes: TYPES, tickRate: 20 });
  runtime.issueCommand({
    owner: 0,
    unitIds: ["u1"],
    type: "move",
    destination: { x: 1, y: 0 },
  });
  runtime.issueCommand({
    owner: 0,
    unitIds: ["u1"],
    type: "move",
    destination: { x: 2, y: 0 },
    queued: true,
  });
  runtime.step(30);
  assert.ok(state.units[0].worldX > 1);
  runtime.step(20);
  assert.ok(Math.abs(state.units[0].worldX - 2) < 0.1);
  assert.equal(state.orders.length, 0);
});

test("units acquire visible enemies and fire on cooldown", () => {
  const state = stateWith([unit("a", 0, 0, 0), unit("b", 1, 3, 0)]);
  const runtime = new RtsRuntime({
    state,
    unitTypes: TYPES,
    tickRate: 20,
    seed: "deterministic",
  });
  runtime.step(1);
  assert.equal(state.units[0].acquiredTargetId, "b");
  runtime.step(20);
  const target = state.units.find((item) => item.id === "b");
  assert.ok(!target || target.hp < 10);
  assert.ok(runtime.drainEvents().some((event) => event.type === "weapon-fired"));
});

test("projectile damage resolves at the locked aim coordinate, not by target id", () => {
  const attacker = unit("a", 0, 0, 0, "projectile");
  const target = unit("b", 1, 4, 0, "medic");
  const state = stateWith([attacker, target]);
  const runtime = new RtsRuntime({ state, unitTypes: TYPES, tickRate: 20 });
  runtime.step(2);
  assert.equal(state.projectiles.length, 1);
  target.worldY = 1;
  target.y = 1;
  runtime.step(25);
  assert.equal(target.hp, TYPES.medic.lives);
  const impact = runtime
    .drainEvents()
    .find((event) => event.type === "projectile-impact");
  assert.deepEqual(impact.damagedEntityIds, []);
});

test("area weapons damage hostiles currently inside their impact radius", () => {
  const attacker = unit("gun", 0, 0, 0, "splash");
  const primary = unit("primary", 1, 5, 0, "medic");
  const nearby = unit("nearby", 1, 5.8, 0, "medic");
  const friendly = unit("friendly", 0, 5.4, 0, "medic");
  const state = stateWith([attacker, primary, nearby, friendly]);
  const runtime = new RtsRuntime({ state, unitTypes: TYPES, tickRate: 20 });
  runtime.step(15);
  assert.equal(primary.hp, TYPES.medic.lives - TYPES.splash.damage);
  assert.equal(nearby.hp, TYPES.medic.lives - TYPES.splash.damage);
  assert.equal(friendly.hp, TYPES.medic.lives);
});

test("hold-fire stance suppresses automatic attacks", () => {
  const attacker = unit("a", 0, 0, 0);
  attacker.stance = "hold-fire";
  const state = stateWith([attacker, unit("b", 1, 2, 0)]);
  const runtime = new RtsRuntime({ state, unitTypes: TYPES, tickRate: 20 });
  runtime.step(60);
  assert.equal(state.units.find((item) => item.id === "b").hp, 10);
});

test("behavior controls switch between attack, flee, and hold-fire", () => {
  const state = stateWith([unit("a", 0, 3, 0), unit("b", 1, 0, 0)]);
  const runtime = new RtsRuntime({ state, unitTypes: TYPES, tickRate: 20 });
  runtime.issueCommand({
    owner: 0,
    unitIds: ["a"],
    type: "hold-fire",
  });
  runtime.step(20);
  assert.equal(state.units.find((item) => item.id === "b").hp, 10);

  const beforeFlee = state.units.find((item) => item.id === "a").worldX;
  runtime.issueCommand({ owner: 0, unitIds: ["a"], type: "flee" });
  runtime.step(20);
  assert.ok(state.units.find((item) => item.id === "a").worldX > beforeFlee);

  runtime.issueCommand({
    owner: 0,
    unitIds: ["a"],
    type: "attack-stance",
  });
  assert.equal(state.units.find((item) => item.id === "a").stance, "attack");
});

test("movement and actions never consume player resources", () => {
  const state = stateWith([unit("a", 0, 0, 0), unit("b", 1, 3, 0)]);
  state.players[0].resources = 25;
  const runtime = new RtsRuntime({ state, unitTypes: TYPES, tickRate: 20 });
  runtime.issueCommand({
    owner: 0,
    unitIds: ["a"],
    type: "move",
    destination: { x: 1, y: 0 },
  });
  runtime.step(30);
  assert.equal(state.players[0].resources, 25);
});

test("simulation continues accepting movement after a unit arrives", () => {
  const state = stateWith([unit("a", 0, 0, 0)]);
  const runtime = new RtsRuntime({ state, unitTypes: TYPES, tickRate: 20 });
  runtime.issueCommand({
    owner: 0,
    unitIds: ["a"],
    type: "move",
    destination: { x: 1, y: 0 },
  });
  runtime.step(30);
  const tickAfterArrival = state.simulationTick;
  runtime.issueCommand({
    owner: 0,
    unitIds: ["a"],
    type: "move",
    destination: { x: 2, y: 0 },
  });
  runtime.step(20);
  assert.equal(state.simulationTick, tickAfterArrival + 20);
  assert.ok(state.units[0].worldX > 1.5);
});

test("units face their movement direction using the model forward axis", () => {
  const state = stateWith([unit("a", 0, 0, 0)]);
  const runtime = new RtsRuntime({ state, unitTypes: TYPES, tickRate: 20 });
  runtime.issueCommand({
    owner: 0,
    unitIds: ["a"],
    type: "move",
    destination: { x: 0, y: 2 },
  });
  runtime.step(2);
  assert.ok(Math.abs(state.units[0].heading + Math.PI / 2) < 0.05);
});

test("aircraft move continuously and consume distance-based fuel", () => {
  const aircraft = unit("aircraft", 0, 0, 0, "plane");
  aircraft.homeAirfieldId = "base";
  const state = stateWith([aircraft]);
  state.structures.push({
    id: "base",
    type: "airfield",
    owner: 0,
    x: 0,
    y: 0,
    hp: 3,
  });
  const runtime = new RtsRuntime({
    state,
    unitTypes: TYPES,
    buildingTypes: { airfield: { lives: 3 } },
    tickRate: 20,
  });
  assert.deepEqual(
    runtime.issueCommand({
      owner: 0,
      unitIds: ["aircraft"],
      type: "move",
      destination: { x: 10, y: 0 },
    }),
    ["aircraft"],
  );
  runtime.step(20);
  assert.equal(aircraft.airborne, true);
  assert.ok(aircraft.worldX > 1.5);
  assert.ok(aircraft.fuel < aircraft.maxFuel);
});

test("aircraft automatically divert to the nearest reachable airfield", () => {
  const aircraft = unit("aircraft", 0, 10, 0, "plane");
  aircraft.airborne = true;
  aircraft.airState = "active";
  aircraft.fuel = 14;
  aircraft.maxFuel = 40;
  aircraft.path = [{ x: 20, y: 0 }];
  const state = stateWith([aircraft]);
  state.structures.push({
    id: "base",
    type: "airfield",
    owner: 0,
    x: 0,
    y: 0,
    hp: 3,
  });
  const runtime = new RtsRuntime({
    state,
    unitTypes: TYPES,
    buildingTypes: { airfield: { lives: 3 } },
    tickRate: 20,
  });
  runtime.step(1);
  assert.equal(aircraft.airState, "returning");
  assert.equal(aircraft.homeAirfieldId, "base");
  assert.equal(aircraft.pathDestination.x, 0);
});

test("landed aircraft refuel fully and await new orders", () => {
  const aircraft = unit("aircraft", 0, 0.1, 0, "plane");
  aircraft.airborne = true;
  aircraft.airState = "returning";
  aircraft.fuel = 20;
  aircraft.maxFuel = 40;
  const state = stateWith([aircraft]);
  state.structures.push({
    id: "base",
    type: "airfield",
    owner: 0,
    x: 0,
    y: 0,
    hp: 3,
  });
  const runtime = new RtsRuntime({
    state,
    unitTypes: TYPES,
    buildingTypes: { airfield: { lives: 3 } },
    tickRate: 20,
  });
  runtime.step(1);
  assert.equal(aircraft.airState, "refueling");
  runtime.step(40);
  assert.equal(aircraft.airState, "landed");
  assert.equal(aircraft.fuel, aircraft.maxFuel);
  assert.equal(state.orders.length, 0);
});

test("aircraft crash when fuel is exhausted without an airfield", () => {
  const aircraft = unit("aircraft", 0, 2, 0, "plane");
  aircraft.airborne = true;
  aircraft.airState = "active";
  aircraft.fuel = 0.08;
  aircraft.maxFuel = 40;
  aircraft.path = [{ x: 5, y: 0 }];
  const state = stateWith([aircraft]);
  const runtime = new RtsRuntime({ state, unitTypes: TYPES, tickRate: 20 });
  runtime.step(3);
  assert.equal(state.units.some((item) => item.id === "aircraft"), false);
  assert.ok(
    runtime
      .drainEvents()
      .some((event) => event.type === "aircraft-crashed"),
  );
});

test("artillery remains stationary while setting up and firing", () => {
  const artillery = unit("gun", 0, 0, 0, "artillery");
  const target = unit("target", 1, 5, 0);
  const state = stateWith([artillery, target]);
  const runtime = new RtsRuntime({
    state,
    unitTypes: TYPES,
    tickRate: 20,
  });
  runtime.issueCommand({
    owner: 0,
    unitIds: ["gun"],
    type: "attack-move",
    destination: { x: 7, y: 0 },
  });
  runtime.step(6);
  const lockedPosition = state.units.find((item) => item.id === "gun").worldX;
  runtime.step(20);
  const gun = state.units.find((item) => item.id === "gun");
  assert.equal(gun.worldX, lockedPosition);
  assert.equal(gun.firingLocked, true);
  assert.ok(Math.abs(gun.heading) < 0.05);
  assert.ok(
    runtime
      .drainEvents()
      .some(
        (event) => event.type === "weapon-fired" && event.actorId === "gun",
      ),
  );
});

test("medical support heals nearby damaged units once per second", () => {
  const wounded = unit("a", 0, 0, 0);
  wounded.hp = 5;
  const state = stateWith([wounded, unit("m", 0, 1, 0, "medic")]);
  const runtime = new RtsRuntime({ state, unitTypes: TYPES, tickRate: 20 });
  runtime.step(20);
  assert.equal(state.units.find((item) => item.id === "a").hp, 6);
});

test("simulation is deterministic for identical commands and seed", () => {
  const run = () => {
    const state = stateWith([unit("a", 0, 0, 0), unit("b", 1, 3, 0)]);
    const runtime = new RtsRuntime({
      state,
      unitTypes: TYPES,
      tickRate: 20,
      seed: "same-seed",
    });
    runtime.step(80);
    return JSON.stringify(state);
  };
  assert.equal(run(), run());
});

test("production jobs complete on their authoritative tick", () => {
  const state = stateWith([unit("a", 0, 0, 0)]);
  state.jobs.push({
    id: "job-1",
    kind: "recruit",
    owner: 0,
    type: "rifle",
    durationTicks: 5,
  });
  const runtime = new RtsRuntime({ state, unitTypes: TYPES, tickRate: 20 });
  runtime.step(4);
  assert.equal(state.jobs.length, 1);
  runtime.step(1);
  assert.equal(state.jobs.length, 0);
  assert.ok(
    runtime
      .drainEvents()
      .some(
        (event) =>
          event.type === "production-complete" && event.job.id === "job-1",
      ),
  );
});

test("reinforcements spawned mid-simulation are normalized before movement", () => {
  const state = stateWith([unit("a", 0, 0, 0)]);
  state.jobs.push({
    id: "job-spawn",
    kind: "recruit",
    owner: 0,
    type: "rifle",
    durationTicks: 1,
  });
  const runtime = new RtsRuntime({
    state,
    unitTypes: TYPES,
    tickRate: 20,
    onEvent: (event) => {
      if (event.type === "production-complete")
        state.units.push(unit("reinforcement", 0, 1, 0));
    },
  });
  runtime.step(2);
  const reinforcement = state.units.find(
    (item) => item.id === "reinforcement",
  );
  assert.deepEqual(reinforcement.path, []);
  assert.equal(reinforcement.pathIndex, 0);
  assert.equal(reinforcement.worldX, 1);
});

test("command posts capture continuously and unlock timed objective victory", () => {
  const state = stateWith([unit("a", 0, 0, 0)]);
  state.structures.push({
    id: "cp",
    type: "objective",
    owner: -1,
    x: 1,
    y: 0,
    hp: 3,
  });
  const runtime = new RtsRuntime({
    state,
    unitTypes: TYPES,
    buildingTypes: { objective: { lives: 3 } },
    tickRate: 20,
  });
  runtime.step(900);
  assert.equal(state.structures[0].owner, 0);
  assert.equal(state.objectiveWinner, 0);
  assert.equal(state.winner, 0);
});

test("annihilating the opposing force ends the real-time match", () => {
  const attacker = unit("a", 0, 0, 0);
  attacker.damage = 10;
  const state = stateWith([attacker, unit("b", 1, 2, 0)]);
  const runtime = new RtsRuntime({
    state,
    unitTypes: TYPES,
    tickRate: 20,
  });
  runtime.step(10);
  assert.equal(state.winner, 0);
  assert.equal(state.units.some((item) => item.id === "b"), false);
});
