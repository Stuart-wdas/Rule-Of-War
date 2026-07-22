import { BUILDINGS, UNIT_TYPES } from "../game-rules.js";
import { createStructure, createUnit } from "../state/game-state.js";

/** Completes queued buildings and recruits at the end of a combat round. */
export class ProductionController {
  constructor({
    getState,
    getReachableTiles,
    pieceAt,
    isFootprintClear,
    getBuildingFootprint,
    distance,
    getLocalOwner,
    coord,
    log,
  }) {
    this.getState = getState;
    this.getReachableTiles = getReachableTiles;
    this.pieceAt = pieceAt;
    this.isFootprintClear = isFootprintClear;
    this.getBuildingFootprint = getBuildingFootprint;
    this.distance = distance;
    this.getLocalOwner = getLocalOwner;
    this.coord = coord;
    this.log = log;
  }

  completeJobs() {
    const state = this.getState();
    const localOwner = this.getLocalOwner();
    const buildingJobs = state.jobs.filter((item) => item.kind === "building");
    for (const job of buildingJobs) {
      const isBridge = job.type === "bridge";
      const footprint =
        job.footprint ?? this.getBuildingFootprint(job.x, job.y);
      if (
        this.isFootprintClear(footprint, {
          ignoreJobId: job.id,
          expectedSize: isBridge
            ? null
            : (BUILDINGS[job.type]?.footprintSize ?? 3) ** 2,
        })
      ) {
        const structure = createStructure(
          job.type,
          job.owner,
          job.x,
          job.y,
          `s-${job.id}`,
          !isBridge,
          footprint,
        );
        if (isBridge)
          Object.assign(structure, {
            endX: job.endX,
            endY: job.endY,
            lengthMeters: job.lengthMeters,
          });
        state.structures.push(structure);
        if (job.owner === localOwner)
          this.log(
            isBridge
              ? `${BUILDINGS.bridge.label} is now operational from ${this.coord(job.x, job.y)} to ${this.coord(job.endX, job.endY)}.`
              : `${BUILDINGS[job.type].label} is now operational at ${this.coord(job.x, job.y)}.`,
          );
      } else {
        state.players[job.owner].resources += job.cost;
        if (job.owner === localOwner)
          this.log(
            `${isBridge ? "Bridge construction" : `Construction at ${this.coord(job.x, job.y)}`} was blocked; supply refunded.`,
            true,
          );
      }
    }

    const recruitJobs = state.jobs.filter((item) => item.kind === "recruit");
    for (const job of recruitJobs) {
      const producer = state.structures.find(
        (item) => item.id === (job.producerId ?? job.barracksId),
      );
      const isAirUnit = UNIT_TYPES[job.type]?.airUnit === true;
      const spawn = producer
        ? isAirUnit
          ? { x: producer.x, y: producer.y }
          : this.findSpawnTile(producer)
        : null;
      if (spawn) {
        const recruited = createUnit(
          job.type,
          job.owner,
          spawn.x,
          spawn.y,
          `u-${job.id}`,
        );
        if (isAirUnit) {
          const occupiedSlots = new Set(
            state.units
              .filter(
                (unit) =>
                  unit.homeAirfieldId === producer.id &&
                  Number.isInteger(unit.parkingSlot),
              )
              .map((unit) => unit.parkingSlot),
          );
          const parkingSlot = [0, 1, 2].find(
            (slot) => !occupiedSlots.has(slot),
          ) ?? 0;
          recruited.homeAirfieldId = producer.id;
          recruited.parkingSlot = parkingSlot;
          recruited.worldX = producer.x + [0.1, 1, 1.9][parkingSlot];
          recruited.worldY = producer.y + 0.8;
          recruited.airborne = false;
          recruited.sortie = null;
        }
        state.units.push(recruited);
        if (job.owner === localOwner)
          this.log(
            `${UNIT_TYPES[job.type].label} reinforcement reported at ${this.coord(spawn.x, spawn.y)}.`,
          );
      } else {
        state.players[job.owner].resources += job.cost;
        if (job.owner === localOwner)
          this.log(
            `${UNIT_TYPES[job.type].label} deployment failed; supply refunded.`,
            true,
          );
      }
    }
  }

  findSpawnTile(producer) {
    const candidates = this.getReachableTiles(producer, 3)
      .filter((tile) => !this.pieceAt(tile.x, tile.y, false))
      .map((tile) => ({
        x: tile.x,
        y: tile.y,
        d: this.distance(producer, tile),
      }));
    candidates.sort(
      (a, b) =>
        a.d - b.d || (producer.owner === 0 ? b.x - a.x : a.x - b.x),
    );
    return candidates[0] || null;
  }
}
