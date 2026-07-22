import { BUILDINGS, UNIT_TYPES } from "../game-rules.js";
import { deploymentCells } from "../core/board-utils.js";

/** Creates a serializable unit. Per-piece fields can override catalog values. */
export function createUnit(
  type,
  owner,
  x,
  y,
  id,
  revealsFog = true,
  movementRange = UNIT_TYPES[type].movementRange,
  damage = UNIT_TYPES[type].damage,
) {
  return {
    id,
    type,
    owner,
    x,
    y,
    hp: UNIT_TYPES[type].lives,
    revealsFog,
    movementRange,
    damage,
    movedThisRound: false,
    attackActionsUsed: 0,
  };
}

/** Creates a serializable structure. */
export function createStructure(
  type,
  owner,
  x,
  y,
  id,
  revealsFog = true,
  footprint = [{ x, y }],
) {
  return {
    id,
    type,
    owner,
    x,
    y,
    hp: BUILDINGS[type].lives,
    revealsFog,
    footprint,
  };
}

export function createStartingForces({
  config,
  cols,
  rows,
  createId,
  deploymentByOwner,
  getBuildingFootprint,
}) {
  const structures = [];
  const units = [];
  const reservedTiles = new Set();
  const keyOf = ({ x, y }) => `${x},${y}`;
  const reserveWithPerimeter = (tiles) => {
    for (const tile of tiles)
      for (let offsetY = -1; offsetY <= 1; offsetY += 1)
        for (let offsetX = -1; offsetX <= 1; offsetX += 1)
          reservedTiles.add(`${tile.x + offsetX},${tile.y + offsetY}`);
  };
  for (const owner of [0, 1]) {
    const prefix = owner === 0 ? "player" : "enemy";
    const cells = deploymentByOwner?.[owner] ?? deploymentCells(cols, rows, owner);
    let cursor = 0;
    const nextStructureCell = () => {
      while (cursor < cells.length) {
        const cell = cells[cursor++];
        const footprint =
          getBuildingFootprint?.(cell.x, cell.y) ?? [{ x: cell.x, y: cell.y }];
        if (
          footprint.length === 9 &&
          footprint.every((tile) => !reservedTiles.has(keyOf(tile)))
        ) {
          reserveWithPerimeter(footprint);
          return { cell, footprint };
        }
      }
      return null;
    };
    const nextUnitCell = () => {
      while (cursor < cells.length) {
        const cell = cells[cursor++];
        if (reservedTiles.has(keyOf(cell))) continue;
        reserveWithPerimeter([cell]);
        return cell;
      }
      return null;
    };
    const addStructures = (type, count) => {
      for (let index = 0; index < count; index += 1) {
        const placement = nextStructureCell();
        if (!placement) break;
        const { cell, footprint } = placement;
        structures.push(
          createStructure(
            type,
            owner,
            cell.x,
            cell.y,
            createId("s"),
            true,
            footprint,
          ),
        );
      }
    };
    const addUnits = (type, count) => {
      for (let index = 0; index < count; index += 1) {
        const cell = nextUnitCell();
        if (!cell) break;
        units.push(createUnit(type, owner, cell.x, cell.y, createId("u")));
      }
    };
    addStructures("barracks", config[`${prefix}Barracks`]);
    addStructures("mine", config[`${prefix}Mines`]);
    addUnits("commander", 1);
    addUnits("short", config[`${prefix}Short`]);
    addUnits("medium", config[`${prefix}Medium`]);
    addUnits("long", config[`${prefix}Long`]);
    addUnits("xtralong", config[`${prefix}Xtralong`]);
  }
  return { structures, units };
}

/** Builds a complete fresh campaign state without touching the DOM. */
export function createFreshState({
  config,
  cols,
  rows,
  createId,
  deploymentByOwner,
  getBuildingFootprint,
}) {
  const startingForces = createStartingForces({
    config,
    cols,
    rows,
    createId,
    deploymentByOwner,
    getBuildingFootprint,
  });
  return {
    round: 1,
    phase: "deployment",
    resolving: false,
    resolutionStep: null,
    winner: null,
    selectedId: null,
    selectedIds: [],
    intent: null,
    buildMode: null,
    bridgeStart: null,
    players: [
      { id: 0, name: "FIELD COMMAND", resources: config.playerSupply },
      { id: 1, name: "IRON FRONT", resources: config.enemySupply },
    ],
    structures: startingForces.structures,
    units: startingForces.units,
    jobs: [],
    orders: [],
    logs: [
      {
        text: `Command established on a ${cols} by ${rows} sector.`,
        danger: false,
      },
      { text: "Enemy signal detected on the eastern line.", danger: true },
    ],
  };
}
