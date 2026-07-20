/**
 * Central gameplay catalog.
 *
 * Add or balance units and buildings here. Rendering may still need a matching
 * visual branch in battlefield3d.js when a completely new piece type is added.
 */
export const UNIT_TYPES = {
  short: {
    label: "SHORT",
    price: 8,
    actionCost: 5,
    range: 20,
    hitChance: 5 / 6,
    lives: 3,
    damage: 1,
    mark: "S",
    movementRange: 10,
  },
  medium: {
    label: "MEDIUM",
    price: 10,
    actionCost: 3,
    range: 32,
    hitChance: 3 / 6,
    lives: 2,
    damage: 2,
    mark: "M",
    movementRange: 10,
  },
  long: {
    label: "LONG",
    price: 14,
    actionCost: 5,
    range: 72,
    hitChance: 1 / 6,
    lives: 1,
    damage: 3,
    mark: "L",
    movementRange: 5,
  },
  xtralong: {
    label: "XTRA-LONG",
    price: 28,
    actionCost: 20,
    range: 144,
    hitChance: 2 / 6,
    lives: 1,
    damage: 3,
    mark: "XL",
    movementRange: 3,
  },
  commander: {
    label: "COMMANDER",
    recruitable: false,
    range: 10,
    actionCost: 0,
    hitChance: 6 / 6,
    lives: 1,
    damage: 3,
    totalAttackActions: 3,
    mark: "CMDR",
    movementRange: 8,
  },
  plane: {
    label: "BOMBER",
    price: 40,
    actionCost: 20,
    range: 156,
    hitChance: 2 / 6,
    lives: 1,
    damage: 3,
    mark: "BMB",
    movementRange: 156,
    productionBuilding: "airfield",
    airUnit: true,
    automaticAttack: true,
  },
};

export const BUILDINGS = {
  mine: {
    label: "SUPPLY MINE",
    price: 25,
    lives: 1,
    mark: "M",
    // Structures reveal the same radius as the commander, but construction
    // expands from one circle centred on the structure's 3 x 3 footprint.
    range: UNIT_TYPES.commander.range,
    buildRange: 9,
    footprintSize: 3,
  },
  barracks: {
    label: "BARRACKS",
    price: 25,
    lives: 1,
    mark: "B",
    range: UNIT_TYPES.commander.range,
    buildRange: 9,
    footprintSize: 3,
  },
  airfield: {
    label: "AIRFIELD",
    price: 25,
    lives: 1,
    mark: "AF",
    range: UNIT_TYPES.commander.range,
    buildRange: 9,
    footprintSize: 3,
  },
  bridge: {
    label: "BRIDGE",
    pricePerMeter: 3,
    lives: 1,
    mark: "BR",
    range: 0,
    buildRange: 0,
    footprintSize: 1,
    commanderOnly: true,
    linear: true,
  },
};

export function bridgeCostForLength(
  lengthMeters,
  pricePerMeter = BUILDINGS.bridge.pricePerMeter,
) {
  const length = Number(lengthMeters);
  const rate = Number(pricePerMeter);
  if (!Number.isFinite(length) || !Number.isFinite(rate) || length <= 0)
    return 0;
  return Math.max(1, Math.round(length * rate));
}

export const DEFAULT_GAME_CONFIG = Object.freeze({
  cols: 48,
  rows: 48,
  playerSupply: 25,
  enemySupply: 25,
  mineIncome: 5,
  playerMines: 1,
  enemyMines: 1,
  playerBarracks: 1,
  enemyBarracks: 1,
  playerShort: 0,
  enemyShort: 1,
  playerMedium: 0,
  enemyMedium: 1,
  playerLong: 0,
  enemyLong: 1,
  playerXtralong: 0,
  enemyXtralong: 0,
  shortRange: 5,
  mediumRange: 8,
  longRange: 14,
  xtralongRange: 28,
  fogEnabled: true,
  terrainDebug: false,
  terrainDebugElevation: false,
});

export const CONFIG_LIMITS = Object.freeze({
  cols: [10, 1000],
  rows: [10, 1000],
  playerSupply: [0, 500],
  enemySupply: [0, 500],
  mineIncome: [0, 50],
  playerMines: [0, 8],
  enemyMines: [0, 8],
  playerBarracks: [0, 8],
  enemyBarracks: [0, 8],
  playerShort: [0, 24],
  enemyShort: [0, 24],
  playerMedium: [0, 24],
  enemyMedium: [0, 24],
  playerLong: [0, 24],
  enemyLong: [0, 24],
  playerXtralong: [0, 24],
  enemyXtralong: [1, 100],
  shortRange: [1, 10],
  mediumRange: [1, 50],
  longRange: [1, 50],
  xtralongRange: [1, 100],
});

/** Applies developer-configurable values to the live unit catalog. */
export function applyConfiguredRules(config) {
  UNIT_TYPES.short.range = config.shortRange;
  UNIT_TYPES.medium.range = config.mediumRange;
  UNIT_TYPES.long.range = config.longRange;
  UNIT_TYPES.xtralong.range = config.xtralongRange;
  return { cols: config.cols, rows: config.rows };
}
