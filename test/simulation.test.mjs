import test from "node:test";
import assert from "node:assert/strict";
import {
  applyDirectCombat,
  planDirectCombat,
} from "../simulation/combat-resolution.js";
import { createEntityIndex } from "../simulation/entity-index.js";
import { resolveMedicalSupport } from "../simulation/support-resolution.js";
import {
  applySectorOutcome,
  canFactionContestSector,
  createWorldState,
  neighboringSectorIds,
  recomputeWorldSupply,
} from "../simulation/world-state.js";
import { validatePlayerPlan } from "../simulation/command-validation.js";
import { createChunkedTerrainSampler } from "../terrain/chunked-terrain.js";
import { resolveObjectives } from "../simulation/objective-resolution.js";
import {
  applyFormationDamage,
  restoreFormationStrength,
} from "../simulation/formation-state.js";
import { worldViewForFaction } from "../simulation/world-interest.js";
import {
  getMapPreset,
  WORLD_CAMPAIGN_MAP_ID,
} from "../maps/map-catalog.js";

const TYPES = {
  rifle: { lives: 3, damage: 1, hitChance: 1 },
  artillery: { lives: 2, damage: 2, hitChance: 1 },
  medic: {
    lives: 2,
    damage: 0,
    hitChance: 0,
    medical: true,
    healing: 1,
    supportRange: 3,
  },
};

function battleState() {
  return {
    sectorId: "sector-1-1",
    units: [
      { id: "a", type: "rifle", owner: 0, x: 1, y: 1, hp: 3 },
      { id: "m", type: "medic", owner: 0, x: 1, y: 2, hp: 2 },
      { id: "b", type: "artillery", owner: 1, x: 2, y: 1, hp: 2 },
    ],
    structures: [],
    jobs: [],
    orders: [
      { id: "oa", kind: "attack", unitId: "a", x: 2, y: 1 },
      { id: "ob", kind: "attack", unitId: "b", x: 1, y: 1 },
    ],
  };
}

test("entity index resolves ids, owners, sectors and occupancy", () => {
  const index = createEntityIndex(battleState());
  assert.equal(index.unit("a").owner, 0);
  assert.equal(index.at(2, 1).id, "b");
  assert.equal(index.owned(0).length, 2);
  assert.equal(index.inSector("sector-1-1").length, 3);
});

test("direct combat is simultaneous and deterministic", () => {
  const state = battleState();
  const plan = planDirectCombat({
    state,
    unitTypes: TYPES,
    random: () => 0,
    resolveTarget: (order, actor, index) => index.at(order.x, order.y),
  });
  assert.deepEqual(
    plan.events.map((event) => [event.actorId, event.targetId, event.damage]),
    [
      ["a", "b", 1],
      ["b", "a", 2],
    ],
  );
  const destroyed = applyDirectCombat(state, plan);
  assert.equal(destroyed.length, 0);
  assert.equal(state.units.find((unit) => unit.id === "a").hp, 1);
  assert.equal(state.units.find((unit) => unit.id === "b").hp, 1);
});

test("medical support automatically heals one nearby friendly", () => {
  const state = battleState();
  state.units[0].hp = 1;
  const events = resolveMedicalSupport(state, TYPES);
  assert.equal(events.length, 1);
  assert.equal(events[0].targetId, "a");
  assert.equal(state.units[0].hp, 2);
});

test("field hospitals use the same automatic support rule", () => {
  const state = battleState();
  state.units[0].hp = 1;
  state.structures = [
    { id: "hospital", type: "fieldHospital", owner: 0, x: 1, y: 3, hp: 2 },
  ];
  state.units = state.units.filter((unit) => unit.type !== "medic");
  const events = resolveMedicalSupport(state, TYPES, {
    buildingTypes: {
      fieldHospital: {
        medicalFacility: true,
        healing: 1,
        supportRange: 5,
      },
    },
  });
  assert.equal(events[0].sourceId, "hospital");
  assert.equal(state.units[0].hp, 2);
});

test("world sectors have stable adjacency and capture events", () => {
  const world = createWorldState({ width: 3, height: 3, seed: "test" });
  assert.equal(neighboringSectorIds(world, "sector-1-1").length, 6);
  const event = applySectorOutcome(world, "sector-1-1", 0);
  assert.equal(event.type, "sector-captured");
  assert.equal(world.sectors.find((item) => item.id === "sector-1-1").owner, 0);
});

test("world campaigns start from one capital and only contest adjacent hexes", () => {
  const world = createWorldState();
  assert.equal(world.sectors.length, 40);
  assert.equal(world.sectors.filter((sector) => sector.owner === 0).length, 1);
  assert.equal(world.sectors.filter((sector) => sector.owner === 1).length, 1);
  const capital = world.sectors.find((sector) => sector.owner === 0);
  const adjacent = neighboringSectorIds(world, capital.id);
  assert.ok(adjacent.length >= 3);
  assert.ok(
    adjacent.every((sectorId) =>
      canFactionContestSector(world, sectorId, 0),
    ),
  );
  assert.equal(canFactionContestSector(world, "sector-5-0", 0), false);
  assert.equal(canFactionContestSector(world, capital.id, 0), false);
});

test("world campaign battles use a 1024 by 1024 tactical map", () => {
  const map = getMapPreset(WORLD_CAMPAIGN_MAP_ID);
  assert.equal(map.cols, 1024);
  assert.equal(map.rows, 1024);
});

test("strategic supply follows contiguous owned sectors automatically", () => {
  const world = createWorldState({ width: 4, height: 1, seed: "supply" });
  world.sectors[1].owner = 0;
  world.sectors[2].owner = 0;
  recomputeWorldSupply(world);
  assert.deepEqual(
    world.sectors.map((sector) => sector.supply),
    [1, 1, 1, 1],
  );
  world.sectors[1].owner = 1;
  recomputeWorldSupply(world);
  assert.deepEqual(
    world.sectors.map((sector) => sector.supply),
    [1, 0, 0, 1],
  );
});

test("world interest management hides distant tactical metadata", () => {
  const world = createWorldState({ width: 6, height: 2, seed: "interest" });
  world.sectors[5].modifications = [{ x: 1, y: 1, patch: { crater: true } }];
  const view = worldViewForFaction(world, 0);
  assert.equal("terrainSeed" in view.sectors[1], true);
  assert.equal("terrainSeed" in view.sectors[5], false);
  assert.equal("modifications" in view.sectors[5], false);
});

test("the server derives resources and ownership from submitted commands", () => {
  const state = battleState();
  state.phase = "attack";
  state.players = [
    { id: 0, resources: 10 },
    { id: 1, resources: 10 },
  ];
  const validated = validatePlayerPlan({
    state,
    playerIndex: 0,
    jobs: [],
    orders: [
      {
        id: "new-attack",
        kind: "attack",
        unitId: "a",
        targetId: "b",
        x: 2,
        y: 1,
        cost: 999,
        owner: 1,
      },
    ],
    unitTypes: {
      ...TYPES,
      rifle: { ...TYPES.rifle, actionCost: 3, range: 5 },
    },
    buildings: {},
    cols: 10,
    rows: 10,
  });
  assert.equal(validated.orders[0].owner, 0);
  assert.equal(validated.orders[0].cost, 3);
  // Replacing the existing 0-cost attack with a 3-cost attack costs 3 supply.
  assert.equal(validated.resources, 7);
});

test("terrain chunks persist sparse world modifications", () => {
  const base = (x, y) => ({
    x,
    y,
    terrainType: "plain",
    movementCost: 1,
    passable: true,
  });
  const terrain = createChunkedTerrainSampler(base, {
    chunkSize: 8,
    maximumResidentChunks: 2,
  });
  terrain.setModification(10, 12, {
    terrainType: "road",
    movementCost: 0.5,
  });
  assert.equal(terrain(10, 12).terrainType, "road");
  assert.equal(terrain(10, 12).movementCost, 0.5);
  assert.deepEqual(terrain.serializeModifications(), [
    {
      x: 10,
      y: 12,
      patch: { terrainType: "road", movementCost: 0.5 },
    },
  ]);
});

test("command posts capture automatically and win after sustained control", () => {
  const state = battleState();
  state.structures = [
    { id: "cp", type: "objective", owner: -1, x: 1, y: 1, hp: 3 },
  ];
  state.units.find((unit) => unit.id === "b").x = 8;
  state.units.find((unit) => unit.id === "b").y = 8;
  state.objectiveControl = { owner: null, rounds: 0 };
  const first = resolveObjectives(state, TYPES, {
    captureRadius: 2,
    roundsToWin: 2,
  });
  assert.equal(first.events[0].owner, 0);
  assert.equal(first.winner, null);
  const second = resolveObjectives(state, TYPES, {
    captureRadius: 2,
    roundsToWin: 2,
  });
  assert.equal(second.winner, 0);
});

test("formation strength scales casualties without extra controls", () => {
  const unit = {
    type: "rifle",
    hp: 3,
    strength: 120,
    maxStrength: 120,
  };
  applyFormationDamage(unit, 1, TYPES);
  assert.equal(unit.hp, 2);
  assert.equal(unit.strength, 80);
  restoreFormationStrength(unit, 1, TYPES);
  assert.equal(unit.strength, 120);
});
