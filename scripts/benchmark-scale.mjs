import { performance } from "node:perf_hooks";
import { createEntityIndex } from "../simulation/entity-index.js";
import { createWorldState, recomputeWorldSupply } from "../simulation/world-state.js";

const unitCount = Number(process.env.BENCHMARK_UNITS) || 100_000;
const state = {
  sectorId: "benchmark",
  units: Array.from({ length: unitCount }, (_, index) => ({
    id: `unit-${index}`,
    type: "short",
    owner: index % 8,
    x: index % 1000,
    y: Math.floor(index / 1000),
    hp: 3,
  })),
  structures: [],
  jobs: [],
  orders: [],
};

const indexStarted = performance.now();
const index = createEntityIndex(state);
const indexDuration = performance.now() - indexStarted;
const queryStarted = performance.now();
for (let indexValue = 0; indexValue < unitCount; indexValue += 100)
  index.unit(`unit-${indexValue}`);
const queryDuration = performance.now() - queryStarted;

const world = createWorldState({
  width: 64,
  height: 64,
  factionCount: 8,
  seed: "benchmark",
});
const supplyStarted = performance.now();
recomputeWorldSupply(world);
const supplyDuration = performance.now() - supplyStarted;

const result = {
  units: unitCount,
  sectors: world.sectors.length,
  entityIndexMs: Number(indexDuration.toFixed(2)),
  indexedLookupBatchMs: Number(queryDuration.toFixed(2)),
  worldSupplyMs: Number(supplyDuration.toFixed(2)),
  heapMb: Number((process.memoryUsage().heapUsed / 1024 / 1024).toFixed(1)),
};
console.log(JSON.stringify(result, null, 2));

if (indexDuration > 2000 || queryDuration > 250 || supplyDuration > 2000) {
  console.error("Scale benchmark exceeded its prototype performance budget.");
  process.exitCode = 1;
}
