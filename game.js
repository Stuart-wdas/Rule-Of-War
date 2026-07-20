import { Battlefield3D } from "./battlefield3d.js";
import { AiController } from "./controllers/ai-controller.js";
import { AirController } from "./controllers/air-controller.js";
import { CombatController } from "./controllers/combat-controller.js";
import { MovementController } from "./controllers/movement-controller.js";
import { ProductionController } from "./controllers/production-controller.js";
import { PhaseController } from "./controllers/phase-controller.js";
import {
  DeveloperConfigStore,
  validateDeveloperConfig,
} from "./config/developer-config.js";
import {
  columnLabel,
  distance as euclideanDistance,
} from "./core/board-utils.js";
import { createBattlefieldViewIndex } from "./core/battlefield-view-index.js";
import {
  BUILDINGS,
  CONFIG_LIMITS,
  DEFAULT_GAME_CONFIG,
  UNIT_TYPES,
  applyConfiguredRules,
  bridgeCostForLength,
} from "./game-rules.js";
import {
  buildNetworkSnapshot,
  cloneJson,
  normalizeMatchCode,
  seededRandom,
  snapshotHash,
} from "./network/snapshot-utils.js";
import {
  createEmptyMultiplayerState,
  websocketAddress,
} from "./network/multiplayer-state.js";
import {
  DEFAULT_MAP_ID,
  MAP_CATALOG,
  applyMapPreset,
  findMapPresetByDimensions,
  getMapPreset,
} from "./maps/map-catalog.js";
import { createRuntimeMap, deploymentTiles } from "./maps/runtime-map.js";
import {
  classifyTerrainTile,
  createTerrainSampler,
  normalizeTerrainType,
  terrainGameplayProperties,
} from "./maps/procedural-terrain.js";
import { findTerrainPath } from "./terrain/terrain-pathfinding.js";
import { createFreshState } from "./state/game-state.js";
import { SavedMatchStore } from "./storage/saved-match-store.js";
import { collectDomElements } from "./ui/dom-elements.js";

let COLS = 48;
let ROWS = 48;
let navigationGraph = null;
let selectedMapId = DEFAULT_MAP_ID;
let activeTerrainSeed = null;
let runtimeMap = null;
let terrainSampler = null;
let terrainGenerator = null;
let terrainTileCache = new Map();

const CONFIG_STORAGE_KEY = "result-of-war:developer-config:v1";
const MATCH_STORAGE_KEY = "result-of-war:saved-matches:v1";
const MAX_DETAILED_TILES = 30000;
const MAX_FALLBACK_TILES = 2000;
const VIRTUAL_RENDER_THRESHOLD = 20000;
const developerConfigStore = new DeveloperConfigStore(CONFIG_STORAGE_KEY);
const savedMatchStore = new SavedMatchStore(MATCH_STORAGE_KEY);
let gameConfig = developerConfigStore.load();
applyRuntimeRules();

const els = collectDomElements();

let idCounter = 0;
let campaignVersion = 0;
let developerReturnFocus = null;
let gameMode = "menu";
let localOwner = 0;
let resolutionRandom = Math.random;
let networkSyncTimer = null;
let reconnectTimer = null;
let intentionalSocketClose = false;
let pendingGameMode = null;
let preparedMapId = null;
let multiplayer = createEmptyMultiplayerState();
const uid = (prefix) =>
  `${prefix}-${localOwner}-${Date.now().toString(36)}-${++idCounter}`;

function enemyOwner() {
  return localOwner === 0 ? 1 : 0;
}

function isHumanGame() {
  return gameMode === "human";
}

function isInteractionLocked() {
  return (
    state.resolving ||
    Boolean(state.winner) ||
    (isHumanGame() &&
      (!multiplayer.connected ||
        !multiplayer.opponentConnected ||
        multiplayer.localReady ||
        multiplayer.synchronizing))
  );
}

function applyRuntimeRules() {
  const dimensions = applyConfiguredRules(gameConfig);
  COLS = dimensions.cols;
  ROWS = dimensions.rows;
  const preset = getMapPreset(selectedMapId);
  runtimeMap = createRuntimeMap(
    preset.cols === COLS && preset.rows === ROWS
      ? preset
      : { id: "custom-grid", name: "CUSTOM GRID", cols: COLS, rows: ROWS },
  );
  activeTerrainSeed ??= runtimeMap.id ?? selectedMapId;
  terrainSampler = createTerrainSampler(
    runtimeMap.bounds,
    activeTerrainSeed,
    preset.terrainConfig,
  );
  terrainGenerator = terrainSampler.terrain;
  terrainTileCache = new Map();
  navigationGraph = runtimeMap.graph;
}

function axisLabels(count, formatLabel) {
  const step = Math.max(1, Math.ceil(count / 25));
  const last = count - 1;
  return Array.from({ length: count }, (_, index) => {
    const visible =
      index === 0 ||
      index === last ||
      (index % step === 0 && last - index >= step);
    return `<span class="${visible ? "axis-label-visible" : "axis-label-minor"}">${visible ? formatLabel(index) : ""}</span>`;
  }).join("");
}

function freshState() {
  const deploymentFor = (owner) =>
    deploymentTiles(
      runtimeMap,
      owner,
      Math.min(runtimeMap.tileCount, Math.max(1024, ROWS * 6)),
    ).filter((tile) => terrainProperties(tile.x, tile.y, tile).passable);
  return createFreshState({
    config: gameConfig,
    cols: COLS,
    rows: ROWS,
    createId: uid,
    deploymentByOwner: [deploymentFor(0), deploymentFor(1)],
    getBuildingFootprint: (x, y) => {
      const footprint = buildingFootprintCoordinates(x, y);
      return footprint.every((tile) => {
        const terrain = terrainProperties(tile.x, tile.y);
        return terrain.passable && !terrain.water;
      })
        ? footprint
        : [];
    },
  });
}

let state = freshState();
let battlefield3D = null;
let battlefieldView = null;
let navigationMarkerHits = [];
let navigationMapProjection = null;
let navigationResizeFrame = null;
let selectedStructureId = null;
let structurePopoverAnchor = null;

// The controller only chooses orders. Every injected engine function still
// validates the AI against the same movement, cost, and combat rules as users.
const aiController = new AiController({
  owner: 1,
  getState: () => state,
  getGameConfig: () => gameConfig,
  // This is only the AI's emergency fallback. Sampling prevents a regular
  // million-tile map from ever being materialized as one giant array.
  getNavigableTiles: () => runtimeMap.sampleTiles(4096),
  getTilesInRange: (origin, range) =>
    runtimeMap.tilesWithinRadius(origin, range),
  distance,
  isValidBuildTile,
  isValidMove,
  queueBuilding,
  queueRecruit,
  queueOrder,
  canUnitAttack,
  attackActionLimitFor,
});

const movementController = new MovementController({
  getUnit,
  pieceAt,
  isGroundOccupied,
  getBattlefield: () => battlefield3D,
  isVisible: isVisibleToPlayer,
  getLocalOwner: () => localOwner,
  getEnemyOwner: enemyOwner,
  coord,
  log,
  pause,
});

const combatController = new CombatController({
  getState: () => state,
  getUnit,
  getAttackTarget,
  canUnitAttack,
  damageFor,
  getBattlefield: () => battlefield3D,
  getLocalOwner: () => localOwner,
  getEnemyOwner: enemyOwner,
  coord,
  log,
  pause,
});

const productionController = new ProductionController({
  getState: () => state,
  getReachableTiles: (origin, range) =>
    runtimeMap
      .tilesWithinRadius(origin, range)
      .filter(
        (tile) =>
          navigationGraph.isConnected(origin, tile) &&
          Number.isFinite(groundMovementCostAt(tile.x, tile.y)),
      ),
  pieceAt,
  isFootprintClear,
  getBuildingFootprint: buildingFootprintCoordinates,
  distance,
  getLocalOwner: () => localOwner,
  coord,
  log,
});

const airController = new AirController({
  getState: () => state,
  getBattlefield: () => battlefield3D,
  getWorldPoint: (piece) =>
    runtimeMap.tileByCoordinate.get(`${piece.x},${piece.y}`)?.center ?? piece,
  getAttackTarget,
  damageFor,
  getLocalOwner: () => localOwner,
  coord,
  log,
});

const phaseController = new PhaseController({
  getState: () => state,
  getCampaignVersion: () => campaignVersion,
  getLocalOwner: () => localOwner,
  aiController,
  movementController,
  airController,
  combatController,
  productionController,
  incomeFor,
  checkWinner,
  render,
  log,
  pause,
});

function coord(x, y) {
  if (runtimeMap?.polygonal) {
    const index = runtimeMap.tileIndexByCoordinate.get(`${x},${y}`) ?? -1;
    return index >= 0 ? `T${String(index + 1).padStart(2, "0")}` : `${x},${y}`;
  }
  return `${columnLabel(x)}${y + 1}`;
}

function distance(a, b) {
  const first = runtimeMap?.tileByCoordinate.get(`${a.x},${a.y}`)?.center ?? a;
  const second = runtimeMap?.tileByCoordinate.get(`${b.x},${b.y}`)?.center ?? b;
  return euclideanDistance(first, second);
}

function completedBridgeAt(x, y) {
  return state?.structures?.some(
    (structure) =>
      structure.type === "bridge" &&
      structure.hp > 0 &&
      occupiesTile(structure, x, y),
  );
}

function groundMovementCostAt(x, y) {
  if (completedBridgeAt(x, y)) return 1;
  const terrain = terrainProperties(x, y);
  return terrain.passable && !terrain.water
    ? terrain.movementCost
    : Number.POSITIVE_INFINITY;
}

function movementRoutePlan(actor, x, y) {
  const destination = navigationGraph.resolve({ x, y });
  const current = navigationGraph.resolve(actor);
  if (!destination || !current) return null;
  const result = findTerrainPath({
    graph: navigationGraph,
    start: current,
    goal: destination,
    movementCostAt: (tile) => groundMovementCostAt(tile.x, tile.y),
    distance,
    maximumVisited: Math.max(10000, Math.min(runtimeMap.tileCount, 1000000)),
  });
  if (!result || result.path.length < 2) return null;

  const movementRange = movementRangeFor(actor);
  const path = result.path.slice(1).map((tile) => ({ x: tile.x, y: tile.y }));
  const waypoints = [];
  let turnCost = 0;
  for (let index = 1; index < result.path.length; index += 1) {
    const previous = result.path[index - 1];
    const currentTile = result.path[index];
    const stepCost =
      distance(previous, currentTile) *
      groundMovementCostAt(currentTile.x, currentTile.y);
    if (turnCost > 0 && turnCost + stepCost > movementRange + 1e-6) {
      const stop = result.path[index - 1];
      waypoints.push({ x: stop.x, y: stop.y });
      turnCost = 0;
    }
    turnCost += stepCost;
    if (turnCost >= movementRange - 1e-6) {
      waypoints.push({ x: currentTile.x, y: currentTile.y });
      turnCost = 0;
    }
  }
  const final = path[path.length - 1];
  const lastWaypoint = waypoints[waypoints.length - 1];
  if (!lastWaypoint || lastWaypoint.x !== final.x || lastWaypoint.y !== final.y)
    waypoints.push({ ...final });
  return { path, waypoints, totalMovementCost: result.cost };
}

function buildingFootprintTiles(x, y) {
  const square = [];
  for (let offsetY = -1; offsetY <= 1; offsetY += 1)
    for (let offsetX = -1; offsetX <= 1; offsetX += 1) {
      const tile = navigationGraph.resolve({ x: x + offsetX, y: y + offsetY });
      if (tile) square.push(tile);
    }
  if (square.length === 9) return square;

  // Authored mixed-topology maps may not use row/column coordinates. On those
  // boards a building claims the centre tile plus its eight nearest connected
  // neighbours, retaining the same nine-tile gameplay footprint.
  if (runtimeMap.virtual || runtimeMap.layout === "hex-odd-q") return [];
  const origin = navigationGraph.resolve({ x, y });
  if (!origin) return [];
  const footprint = [origin];
  const visited = new Set([origin.id]);
  for (let cursor = 0; cursor < footprint.length && footprint.length < 9; cursor += 1) {
    const neighbors = navigationGraph
      .neighbors(footprint[cursor])
      .filter((tile) => !visited.has(tile.id))
      .sort((first, second) => distance(origin, first) - distance(origin, second));
    for (const tile of neighbors) {
      visited.add(tile.id);
      footprint.push(tile);
      if (footprint.length === 9) break;
    }
  }
  return footprint.length === 9 ? footprint : [];
}

function buildingFootprintCoordinates(x, y) {
  return buildingFootprintTiles(x, y).map((tile) => ({ x: tile.x, y: tile.y }));
}

function bridgeMetrics(start, end) {
  const lengthMeters = distance(start, end);
  return {
    lengthMeters,
    cost: bridgeCostForLength(lengthMeters),
  };
}

function bridgeFootprintCoordinates(start, end) {
  const startTile = navigationGraph.resolve(start);
  const endTile = navigationGraph.resolve(end);
  if (!startTile || !endTile) return [];
  const startCenter = startTile.center ?? startTile;
  const endCenter = endTile.center ?? endTile;
  const lengthMeters = euclideanDistance(startCenter, endCenter);
  if (lengthMeters <= 1e-6) return [{ x: startTile.x, y: startTile.y }];

  const candidates = runtimeMap.tilesWithinRadius(
    startTile,
    lengthMeters + 1.5,
    { includeOrigin: true },
  );
  if (!candidates.some((tile) => tile.x === endTile.x && tile.y === endTile.y))
    candidates.push(endTile);
  const footprintByKey = new Map();
  const sampleCount = Math.max(2, Math.ceil(lengthMeters * 4));
  for (let index = 0; index <= sampleCount; index += 1) {
    const amount = index / sampleCount;
    const sample = {
      x: startCenter.x + (endCenter.x - startCenter.x) * amount,
      y: startCenter.y + (endCenter.y - startCenter.y) * amount,
    };
    const nearest = candidates.reduce((best, tile) => {
      const center = tile.center ?? tile;
      const tileDistance = euclideanDistance(center, sample);
      return !best || tileDistance < best.distance
        ? { tile, distance: tileDistance }
        : best;
    }, null)?.tile;
    if (nearest)
      footprintByKey.set(`${nearest.x},${nearest.y}`, {
        x: nearest.x,
        y: nearest.y,
      });
  }
  footprintByKey.set(`${startTile.x},${startTile.y}`, {
    x: startTile.x,
    y: startTile.y,
  });
  footprintByKey.set(`${endTile.x},${endTile.y}`, {
    x: endTile.x,
    y: endTile.y,
  });
  return [...footprintByKey.values()].sort((first, second) => {
    const firstCenter =
      runtimeMap.tileByCoordinate.get(`${first.x},${first.y}`)?.center ?? first;
    const secondCenter =
      runtimeMap.tileByCoordinate.get(`${second.x},${second.y}`)?.center ??
      second;
    const directionX = endCenter.x - startCenter.x;
    const directionY = endCenter.y - startCenter.y;
    return (
      (firstCenter.x - startCenter.x) * directionX +
      (firstCenter.y - startCenter.y) * directionY -
      ((secondCenter.x - startCenter.x) * directionX +
        (secondCenter.y - startCenter.y) * directionY)
    );
  });
}

function occupiedCoordinates(piece) {
  return Array.isArray(piece?.footprint) && piece.footprint.length
    ? piece.footprint
    : piece
      ? [{ x: piece.x, y: piece.y }]
      : [];
}

function occupiesTile(piece, x, y) {
  return occupiedCoordinates(piece).some((tile) => tile.x === x && tile.y === y);
}

function piecesAt(x, y, includePending = true) {
  const units = state.units.filter((item) => item.x === x && item.y === y);
  const structures = state.structures.filter((item) => occupiesTile(item, x, y));
  const pending = includePending
    ? state.jobs.filter(
        (job) => job.kind === "building" && occupiesTile(job, x, y),
      )
    : [];
  return [...units, ...structures, ...pending];
}

function pieceAt(x, y, includePending = true) {
  return piecesAt(x, y, includePending)[0] ?? null;
}

function isGroundOccupied(x, y, includePending = true) {
  return piecesAt(x, y, includePending).some(
    (piece) =>
      piece.type !== "bridge" &&
      (!(piece.type in UNIT_TYPES) ||
        UNIT_TYPES[piece.type]?.airUnit !== true ||
        piece.airborne !== true),
  );
}

function isFootprintClear(
  footprint,
  { ignoreJobId = null, expectedSize = 9 } = {},
) {
  return (
    Array.isArray(footprint) &&
    footprint.length > 0 &&
    (expectedSize === null || footprint.length === expectedSize) &&
    footprint.every((tile) =>
      piecesAt(tile.x, tile.y).every((piece) => piece.id === ignoreJobId),
    )
  );
}

function getAttackTarget(x, y, actor) {
  const hostile = piecesAt(x, y, false).filter(
    (piece) => piece.owner !== actor.owner && piece.hp > 0,
  );
  return (
    hostile.find(
      (piece) =>
        !(piece.type in UNIT_TYPES) ||
        UNIT_TYPES[piece.type]?.airUnit !== true,
    ) ??
    null
  );
}

function isPieceVisible(piece, view = battlefieldView) {
  return (
    piece.owner === localOwner ||
    occupiedCoordinates(piece).some((tile) => view?.isVisible(tile.x, tile.y))
  );
}

function getUnit(id) {
  return state.units.find((item) => item.id === id);
}

function getStructure(id) {
  return state.structures.find((item) => item.id === id);
}

function movementRangeFor(unitItem) {
  const movementRange =
    unitItem?.movementRange ?? UNIT_TYPES[unitItem?.type]?.movementRange;
  return Number.isInteger(movementRange) && movementRange > 0
    ? movementRange
    : 1;
}

function totalAttackActionLimitFor(unitItem) {
  const limit = UNIT_TYPES[unitItem?.type]?.totalAttackActions;
  return Number.isInteger(limit) && limit > 0 ? limit : 1;
}

function attackActionLimitFor(unitItem) {
  const total = totalAttackActionLimitFor(unitItem);
  const lifetimeLimit = UNIT_TYPES[unitItem?.type]?.totalAttackActions;
  if (!Number.isInteger(lifetimeLimit)) return total;
  const used = Number.isInteger(unitItem?.attackActionsUsed)
    ? unitItem.attackActionsUsed
    : 0;
  return Math.max(0, total - used);
}

function queuedOrderCount(unitItem, kind) {
  if (!unitItem) return 0;
  return state.orders.filter(
    (order) => order.unitId === unitItem.id && order.kind === kind,
  ).length;
}

function canQueueUnitOrder(unitItem, kind) {
  const limit = kind === "attack" ? attackActionLimitFor(unitItem) : 1;
  return queuedOrderCount(unitItem, kind) < limit;
}

function damageFor(unitItem) {
  const damage = unitItem?.damage ?? UNIT_TYPES[unitItem?.type]?.damage;
  return Number.isInteger(damage) && damage > 0 ? damage : 1;
}

function canUnitAttack(unitItem) {
  return (
    Boolean(unitItem) &&
    UNIT_TYPES[unitItem.type]?.automaticAttack !== true &&
    unitItem.movedThisRound !== true
  );
}

function playerBarracks(owner = localOwner) {
  return state.structures.filter(
    (item) => item.owner === owner && item.type === "barracks",
  );
}

function productionBuildingType(type) {
  return UNIT_TYPES[type]?.productionBuilding ?? "barracks";
}

function producerCanRecruit(producer, type, owner = localOwner) {
  if (
    !producer ||
    producer.owner !== owner ||
    producer.type !== productionBuildingType(type) ||
    producer.hp <= 0
  )
    return false;
  if (UNIT_TYPES[type]?.airUnit !== true) return true;
  return (
    !state.units.some(
      (unit) => unit.homeAirfieldId === producer.id && unit.hp > 0,
    ) &&
    !state.jobs.some(
      (job) => job.kind === "recruit" && job.producerId === producer.id,
    )
  );
}

function availableProducer(type, owner = localOwner) {
  return (
    state.structures.find((structure) =>
      producerCanRecruit(structure, type, owner),
    ) ?? null
  );
}

function incomeFor(owner) {
  return (
    state.structures.filter(
      (item) => item.owner === owner && item.type === "mine",
    ).length * gameConfig.mineIncome
  );
}

function isVisibleToPlayer(x, y) {
  if (!gameConfig.fogEnabled) return true;
  const occupant = pieceAt(x, y);
  if (occupant && occupant.owner === localOwner) return true;
  return [...state.units, ...state.structures]
    .filter((piece) => piece.owner === localOwner && piece.revealsFog === true)
    .some((piece) => {
      const spec = UNIT_TYPES[piece.type] ?? BUILDINGS[piece.type];

      return distance(piece, { x, y }) <= spec.range;
    });
}

function log(text, danger = false) {
  state.logs.unshift({ text, danger });
  state.logs = state.logs.slice(0, 8);
}

function syncConfigDependentUI() {
  document.body.classList.toggle("terrain-debug", gameConfig.terrainDebug);
  const polygonMap = runtimeMap.polygonal;
  const materializeAxisLabels = !polygonMap && !isVirtualizedBattlefield();
  els.columnLabels.innerHTML = materializeAxisLabels
    ? axisLabels(COLS, columnLabel)
    : "";
  els.rowLabels.innerHTML = materializeAxisLabels
    ? axisLabels(ROWS, (index) => index + 1)
    : "";
  const battlefieldWrap = els.battlefield.closest(".battlefield-wrap");
  const polygonColumns = Math.max(
    1,
    Math.round(runtimeMap.bounds.maxX - runtimeMap.bounds.minX),
  );
  const polygonRows = Math.max(
    1,
    Math.round(runtimeMap.bounds.maxY - runtimeMap.bounds.minY),
  );
  battlefieldWrap.style.setProperty(
    "--map-cols",
    polygonMap ? polygonColumns : COLS,
  );
  battlefieldWrap.style.setProperty(
    "--map-rows",
    polygonMap ? polygonRows : ROWS,
  );
  battlefieldWrap.style.setProperty(
    "--map-min-height",
    `${Math.max(460, Math.min(580, 420 + ROWS * 3))}px`,
  );
  battlefieldWrap.style.setProperty(
    "--map-view-height",
    `${Math.min(72, 58 + Math.max(0, ROWS - 16) * 0.35)}vh`,
  );
  battlefieldWrap.classList.toggle("large-map", Math.max(COLS, ROWS) > 24);
  battlefieldWrap.classList.toggle("huge-map", Math.max(COLS, ROWS) > 40);
  els.battlefieldGrid.setAttribute(
    "aria-colcount",
    polygonMap ? runtimeMap.tiles.length : COLS,
  );
  els.battlefieldGrid.setAttribute("aria-rowcount", polygonMap ? 1 : ROWS);
  els.mapDimensions.textContent = polygonMap
    ? `${runtimeMap.name} · ${runtimeMap.tiles.length} TILES`
    : `${COLS} \u00d7 ${ROWS} GRID`;

  els.unitDoctrine.innerHTML = Object.entries(UNIT_TYPES)
    .map(
      ([, spec]) => `
    <article class="doctrine-card">
      <span class="doctrine-icon">${spec.mark}</span>
      <span>
        <strong>${spec.label} ${spec.airUnit ? "SORTIE" : "RANGE"}</strong>
        <small>${spec.airUnit ? `${spec.movementRange} FLIGHT · AUTO BOMB · INTERCEPTABLE` : `${spec.range} FIRE · ${spec.movementRange} MOVE`} · ${spec.damage} DMG · ${Math.round(spec.hitChance * 6)}/6 HIT · ${spec.lives} HP${spec.totalAttackActions ? ` · ${spec.totalAttackActions} TOTAL SHOTS` : ""}</small>
        <b>${spec.recruitable === false ? `BUILDS WITHIN ${spec.movementRange}` : spec.airUnit ? `AIRFIELD · ◆ ${spec.actionCost} / RUN` : `◆ ${spec.actionCost} / ACT`}</b>
      </span>
    </article>
  `,
    )
    .join("");

  const moveCopy = els.moveButton.querySelector("small");
  if (moveCopy) moveCopy.textContent = "Select a unit";
  const mineCopy = document.querySelector('[data-build="mine"] small');
  if (mineCopy)
    mineCopy.textContent = `+${gameConfig.mineIncome} supply / round`;
  const movementRule = document.querySelector("#movement-rule");
  if (movementRule)
    movementRule.textContent = Object.values(UNIT_TYPES)
      .map((spec) => `${spec.label} ${spec.movementRange}`)
      .join(" · ");
  els.developerButton.classList.toggle(
    "configured",
    Object.keys(DEFAULT_GAME_CONFIG).some(
      (key) => gameConfig[key] !== DEFAULT_GAME_CONFIG[key],
    ),
  );
}

function setHudPanelCollapsed(panel, collapsed) {
  if (!panel) return;
  panel.classList.toggle("collapsed", collapsed);
  for (const button of panel.querySelectorAll(
    `:scope [data-hud-target="${panel.id}"]`,
  )) {
    const panelLabel =
      button.dataset.hudLabel ||
      button
        .getAttribute("aria-label")
        ?.replace(/^(Collapse|Expand)\s+/i, "") ||
      "panel";
    button.dataset.hudLabel = panelLabel;
    button.setAttribute("aria-expanded", String(!collapsed));
    button.setAttribute(
      "aria-label",
      `${collapsed ? "Expand" : "Collapse"} ${panelLabel}`,
    );
    button.title = `${collapsed ? "Expand" : "Collapse"} ${panelLabel}`;
  }
  window.requestAnimationFrame(() => {
    battlefield3D?.resize?.();
    if (panel.id === "navigation-hud" && !collapsed)
      renderTacticalNavigation(battlefieldView);
  });
}

function initializeHudPanels() {
  const compactViewport = window.matchMedia("(max-width: 760px)").matches;
  const compactDefaults = new Set([
    "mission-hud",
    "map-info-hud",
    "intel-hud",
    "navigation-hud",
  ]);

  document.querySelectorAll("[data-hud-target]").forEach((button) => {
    const panel = document.getElementById(button.dataset.hudTarget);
    if (!panel) return;
    if (compactViewport && compactDefaults.has(panel.id))
      setHudPanelCollapsed(panel, true);
    button.addEventListener("click", () =>
      setHudPanelCollapsed(panel, !panel.classList.contains("collapsed")),
    );
  });
}

function createStaticUI() {
  syncConfigDependentUI();
  initializeHudPanels();

  els.structurePopover.addEventListener("click", (event) => {
    const closeButton = event.target.closest("[data-close-structure]");
    if (closeButton) {
      closeStructurePopover();
      render();
      return;
    }
    const recruitButton = event.target.closest("[data-structure-recruit]");
    if (recruitButton)
      queueRecruit(
        recruitButton.dataset.structureRecruit,
        localOwner,
        selectedStructureId,
      );
  });

  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || !selectedStructureId) return;
    closeStructurePopover();
    render();
  });

  document.querySelectorAll("[data-build]").forEach((button) => {
    button.addEventListener("click", () => chooseBuild(button.dataset.build));
  });

  els.battlefieldGrid.addEventListener("click", (event) => {
    const tile = event.target.closest(".tile");
    if (!tile || isInteractionLocked()) return;
    handleTileClick(Number(tile.dataset.x), Number(tile.dataset.y), {
      clientX: event.clientX,
      clientY: event.clientY,
    });
  });

  els.phaseButton.addEventListener("click", advancePhase);
  els.moveButton.addEventListener("click", () => setIntent("move"));
  els.attackButton.addEventListener("click", () => setIntent("attack"));
  els.orderList.addEventListener("click", (event) => {
    const button = event.target.closest("[data-remove]");
    if (button) removeQueued(button.dataset.remove);
  });
  els.unitQuickNav.addEventListener("click", (event) => {
    const button = event.target.closest("[data-unit-nav]");
    if (button) navigateToUnit(button.dataset.unitNav);
  });
  els.tacticalMap.addEventListener("click", handleTacticalMapClick);
  window.addEventListener("resize", () => {
    if (navigationResizeFrame) cancelAnimationFrame(navigationResizeFrame);
    navigationResizeFrame = requestAnimationFrame(() => {
      navigationResizeFrame = null;
      renderTacticalNavigation(battlefieldView);
    });
  });

  document
    .querySelector("#help-button")
    .addEventListener("click", () => openModal(els.helpModal));
  document
    .querySelector("#close-help")
    .addEventListener("click", () => closeModal(els.helpModal));
  document
    .querySelector("#acknowledge-help")
    .addEventListener("click", () => closeModal(els.helpModal));
  document
    .querySelector("#restart-button")
    .addEventListener("click", restartGame);
  document
    .querySelector("#result-restart")
    .addEventListener("click", restartGame);
  els.developerButton.addEventListener("click", openDeveloperTools);
  els.resetViewButton.addEventListener("click", () =>
    battlefield3D?.resetView(),
  );
  els.developerClose.addEventListener("click", closeDeveloperTools);
  els.developerCancel.addEventListener("click", closeDeveloperTools);
  els.developerReset.addEventListener("click", () =>
    populateDeveloperForm(DEFAULT_GAME_CONFIG),
  );
  els.developerForm.addEventListener("submit", applyDeveloperSettings);
  els.playAiButton.addEventListener("click", () => showMapMenu("ai"));
  els.playHumanButton.addEventListener("click", () => showMapMenu("human"));
  els.mapBackButton.addEventListener("click", showMainMenu);
  els.mapCardGrid.addEventListener("click", handleMapSelection);
  els.mapConfirmButton.addEventListener("click", confirmMapSelection);
  els.humanBackButton.addEventListener("click", () => showMapMenu("human"));
  els.createMatchButton.addEventListener("click", createHumanMatch);
  els.joinMatchForm.addEventListener("submit", joinHumanMatch);
  els.joinMatchCode.addEventListener("input", () => {
    els.joinMatchCode.value = normalizeMatchCode(els.joinMatchCode.value);
    clearJoinError();
  });
  els.savedMatchList.addEventListener("click", (event) => {
    const deleteButton = event.target.closest("[data-delete-code]");
    if (deleteButton) {
      deleteSavedMatch(
        deleteButton.dataset.deleteCode,
        deleteButton.dataset.deleteToken,
      );
      return;
    }
    const button = event.target.closest("[data-resume-code]");
    if (button)
      resumeHumanMatch(button.dataset.resumeCode, button.dataset.resumeToken);
  });
  els.copyMatchCode.addEventListener("click", copyCurrentMatchCode);
  els.cancelLobbyButton.addEventListener("click", leaveHumanMatch);
  els.leaveGameButton.addEventListener("click", leaveHumanMatch);
  els.menuButton.addEventListener("click", () => {
    if (isHumanGame()) leaveHumanMatch();
    else showGameMenu();
  });
  document.addEventListener("keydown", handleGlobalShortcut, true);
  window.addEventListener("beforeunload", () => {
    if (!isHumanGame() || !multiplayer.code || !multiplayer.token) return;
    sendPlanUpdate();
    saveMatchReference();
  });
  els.helpModal.addEventListener("click", (event) => {
    if (event.target === els.helpModal) closeModal(els.helpModal);
  });
  els.developerModal.addEventListener("click", (event) => {
    if (event.target === els.developerModal) closeDeveloperTools();
  });
}

function openModal(modal) {
  modal.hidden = false;
}

function closeModal(modal) {
  modal.hidden = true;
}

function populateDeveloperForm(config = gameConfig) {
  for (const input of els.developerForm.elements) {
    if (!input.name) continue;
    if (input.type === "checkbox") input.checked = Boolean(config[input.name]);
    else input.value = config[input.name];
    input.removeAttribute("aria-invalid");
  }
  els.developerError.hidden = true;
  els.developerError.textContent = "";
}

function openDeveloperTools() {
  developerReturnFocus =
    document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
  populateDeveloperForm();
  els.developerModal.hidden = false;
  document.body.classList.add("devtools-open");
  document.querySelector(".app-shell").inert = true;
  els.developerClose.focus();
}

function closeDeveloperTools() {
  if (els.developerModal.hidden) return;
  els.developerModal.hidden = true;
  document.body.classList.remove("devtools-open");
  document.querySelector(".app-shell").inert = !els.gameMenu.hidden;
  const returnTarget = developerReturnFocus;
  developerReturnFocus = null;
  returnTarget?.focus?.();
}

function trapDeveloperFocus(event) {
  const focusable = [
    ...els.developerModal.querySelectorAll(
      "button:not([disabled]), input:not([disabled])",
    ),
  ].filter((element) => element.getClientRects().length > 0);
  if (!focusable.length) return;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}

function handleGlobalShortcut(event) {
  const developerShortcut =
    event.key === "F12" ||
    (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "d");
  if (developerShortcut) {
    event.preventDefault();
    event.stopPropagation();
    if (els.developerModal.hidden) openDeveloperTools();
    else closeDeveloperTools();
    return;
  }
  if (!els.developerModal.hidden && event.key === "Escape") {
    event.preventDefault();
    closeDeveloperTools();
  } else if (!els.developerModal.hidden && event.key === "Tab") {
    trapDeveloperFocus(event);
  }
}

function readDeveloperForm() {
  const config = {};
  for (const input of els.developerForm.elements) {
    if (!input.name) continue;
    config[input.name] =
      input.type === "checkbox"
        ? input.checked
        : input.value.trim() === ""
          ? Number.NaN
          : Number(input.value);
  }
  return config;
}

function applyDeveloperSettings(event) {
  event.preventDefault();
  if (isHumanGame()) {
    els.developerError.textContent =
      "Leave the human campaign before changing battlefield rules.";
    els.developerError.hidden = false;
    els.developerError.focus();
    return;
  }
  const config = readDeveloperForm();
  const error = validateDeveloperConfig(config);
  for (const input of els.developerForm.querySelectorAll("input"))
    input.removeAttribute("aria-invalid");
  if (error) {
    for (const input of els.developerForm.querySelectorAll(
      'input[type="number"]',
    )) {
      const limits = CONFIG_LIMITS[input.name];
      const value = Number(input.value);
      if (
        limits &&
        (!Number.isInteger(value) || value < limits[0] || value > limits[1])
      ) {
        input.setAttribute("aria-invalid", "true");
      }
    }
    els.developerError.textContent = error;
    els.developerError.hidden = false;
    const invalidInput = els.developerForm.querySelector(
      '[aria-invalid="true"]',
    );
    if (invalidInput) invalidInput.focus();
    else els.developerError.focus();
    return;
  }

  campaignVersion += 1;
  gameConfig = config;
  developerConfigStore.save(gameConfig);
  activeTerrainSeed = null;
  applyRuntimeRules();
  state = freshState();
  els.resultModal.hidden = true;
  els.helpModal.hidden = true;
  syncConfigDependentUI();
  initializeBattlefield3D();
  closeDeveloperTools();
  render();
}

function restartGame() {
  if (isHumanGame()) {
    leaveHumanMatch();
    return;
  }
  campaignVersion += 1;
  localOwner = 0;
  state = freshState();
  els.resultModal.hidden = true;
  render();
}

function createNetworkSnapshot() {
  const snapshot = buildNetworkSnapshot(
    state,
    gameConfig,
    Object.keys(DEFAULT_GAME_CONFIG),
  );
  snapshot.terrain = { seed: activeTerrainSeed };
  return snapshot;
}

function isValidNetworkSnapshot(snapshot) {
  if (!snapshot || snapshot.schema !== 1 || !snapshot.game || !snapshot.config)
    return false;
  if (
    snapshot.terrain?.seed !== undefined &&
    !["string", "number"].includes(typeof snapshot.terrain.seed)
  )
    return false;
  if (validateDeveloperConfig(snapshot.config)) return false;
  const game = snapshot.game;
  return (
    Number.isInteger(game.round) &&
    ["deployment", "orders", "attack"].includes(game.phase) &&
    Array.isArray(game.players) &&
    game.players.length === 2 &&
    Array.isArray(game.structures) &&
    Array.isArray(game.units) &&
    Array.isArray(game.jobs) &&
    Array.isArray(game.orders)
  );
}

function applyNetworkSnapshot(snapshot, { cancelAnimations = true } = {}) {
  if (!isValidNetworkSnapshot(snapshot)) {
    showConnectionError("The shared campaign state was invalid.");
    return false;
  }

  if (cancelAnimations) campaignVersion += 1;
  const previousConfig = JSON.stringify(gameConfig);
  const localUi = {
    selectedId: state.selectedId,
    intent: state.intent,
    buildMode: state.buildMode,
    bridgeStart: state.bridgeStart,
    logs: state.logs,
  };
  gameConfig = { ...DEFAULT_GAME_CONFIG, ...snapshot.config };
  selectedMapId = findMapPresetByDimensions(
    gameConfig.cols,
    gameConfig.rows,
  ).id;
  activeTerrainSeed = snapshot.terrain?.seed ?? null;
  applyRuntimeRules();
  const game = cloneJson(snapshot.game);
  for (const structure of game.structures) {
    if (
      structure.type === "bridge" &&
      Number.isFinite(structure.endX) &&
      Number.isFinite(structure.endY) &&
      (!Array.isArray(structure.footprint) || !structure.footprint.length)
    )
      structure.footprint = bridgeFootprintCoordinates(structure, {
        x: structure.endX,
        y: structure.endY,
      });
    else if (
      structure.type !== "bridge" &&
      (!Array.isArray(structure.footprint) || structure.footprint.length !== 9)
    )
      structure.footprint = buildingFootprintCoordinates(
        structure.x,
        structure.y,
      );
  }
  for (const job of game.jobs) {
    if (
      job.kind === "building" &&
      job.type === "bridge" &&
      Number.isFinite(job.endX) &&
      Number.isFinite(job.endY) &&
      (!Array.isArray(job.footprint) || !job.footprint.length)
    )
      job.footprint = bridgeFootprintCoordinates(job, {
        x: job.endX,
        y: job.endY,
      });
    else if (
      job.kind === "building" &&
      job.type !== "bridge" &&
      (!Array.isArray(job.footprint) || job.footprint.length !== 9)
    )
      job.footprint = buildingFootprintCoordinates(job.x, job.y);
  }
  state = {
    ...game,
    resolving: false,
    resolutionStep: null,
    selectedId: game.units.some(
      (unitItem) =>
        unitItem.id === localUi.selectedId && unitItem.owner === localOwner,
    )
      ? localUi.selectedId
      : null,
    intent: localUi.intent,
    buildMode: localUi.buildMode,
    bridgeStart:
      localUi.buildMode === "bridge" ? localUi.bridgeStart : null,
    logs:
      localUi.logs?.length > 0
        ? localUi.logs
        : [
            {
              text: `Shared command restored in round ${String(game.round).padStart(2, "0")}.`,
              danger: false,
            },
          ],
  };

  if (previousConfig !== JSON.stringify(gameConfig)) {
    syncConfigDependentUI();
    initializeBattlefield3D();
  }
  if (state.winner !== null) showWinnerResult();
  return true;
}

function saveMatchReference(snapshot = createNetworkSnapshot()) {
  if (!multiplayer.code || !multiplayer.token) return;
  savedMatchStore.upsert({
    code: multiplayer.code,
    token: multiplayer.token,
    owner: localOwner,
    round: snapshot.game.round,
    phase: snapshot.game.phase,
    updatedAt: new Date().toISOString(),
    state: snapshot,
  });
  renderSavedMatches();
}

function renderSavedMatches() {
  const saved = savedMatchStore.load();
  els.savedMatchCount.textContent = `${saved.length} SAVED`;
  if (!saved.length) {
    els.savedMatchList.innerHTML =
      '<p class="empty-saves" id="empty-saves">No saved human campaigns on this device.</p>';
    return;
  }
  els.savedMatchList.innerHTML = saved
    .map((item) => {
      const date = new Date(item.updatedAt);
      const dateLabel = Number.isNaN(date.valueOf())
        ? "LOCAL CHECKPOINT"
        : date.toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
      return `<article class="saved-match-card"><span><strong>${item.code} · ROUND ${String(item.round || 1).padStart(2, "0")}</strong><small>${item.owner === 0 ? "HOST COMMAND" : "SECOND COMMAND"} · ${String(item.phase || "deployment").toUpperCase()} · ${dateLabel}</small></span><button type="button" data-resume-code="${item.code}" data-resume-token="${item.token}">RESUME</button><button class="delete-save" type="button" data-delete-code="${item.code}" data-delete-token="${item.token}" aria-label="Remove saved match ${item.code}">×</button></article>`;
    })
    .join("");
}

function deleteSavedMatch(code, token) {
  savedMatchStore.remove(code, token);
  renderSavedMatches();
}

function setMenuView(view) {
  els.menuHome.hidden = view !== "home";
  els.menuMaps.hidden = view !== "maps";
  els.menuLoading.hidden = view !== "loading";
  els.menuHuman.hidden = view !== "human";
  els.lobbyPanel.hidden = view !== "lobby";
}

function showGameMenu(view = "home") {
  els.gameMenu.hidden = false;
  document.querySelector(".app-shell").inert = true;
  setMenuView(view);
  if (view === "human") renderSavedMatches();
}

function showMainMenu() {
  clearJoinError();
  pendingGameMode = null;
  showGameMenu("home");
}

function renderMapOptions() {
  els.mapCardGrid.innerHTML = MAP_CATALOG.map((map, index) => {
    const selected = map.id === selectedMapId;
    const playable = isPlayableMap(map);
    const dimensions = map.tileCount
      ? `${map.tileCount} ${map.layout === "hex-odd-q" ? "HEX" : "MIXED"} TILES`
      : playable
        ? `${map.cols} × ${map.rows}`
        : "INVALID MAP";
    return `
      <button class="map-card${selected ? " selected" : ""}${playable ? "" : " topology-only"}" type="button" data-map-id="${map.id}" aria-pressed="${selected}">
        <span class="map-preview map-preview-${map.preview ?? "topology"}" aria-hidden="true"><i></i><i></i><i></i></span>
        <span class="map-card-copy">
          <small>${String(index + 1).padStart(2, "0")} / ${map.theater ?? "CUSTOM TOPOLOGY"}</small>
          <strong>${map.name}</strong>
          <span>${map.description ?? "A mixed-polygon battlefield awaiting renderer support."}</span>
        </span>
        <b>${dimensions}</b>
      </button>
    `;
  }).join("");
  renderSelectedMap();
}

function isPlayableMap(map) {
  return (
    Number.isInteger(map?.cols) &&
    Number.isInteger(map?.rows) &&
    (!map.tileCount || map.tileCount >= 12)
  );
}

function renderSelectedMap() {
  const map = getMapPreset(selectedMapId);
  const playable = isPlayableMap(map);
  els.mapConfirmButton.disabled = !playable;
  els.selectedMapCopy.textContent = playable
    ? map.tileCount
      ? `${map.name} · ${map.tileCount} TILES SELECTED`
      : `${map.name} · ${map.cols} × ${map.rows} SELECTED`
    : `${map.name} SELECTED · POLYGON RENDERER REQUIRED`;
}

function showMapMenu(mode) {
  pendingGameMode = mode;
  els.mapMenuEyebrow.textContent =
    mode === "human" ? "HOST BATTLEFIELD" : "SOLO BATTLEFIELD";
  els.mapMenuCopy.textContent =
    mode === "human"
      ? "Choose the battlefield used when hosting. Joined and resumed campaigns automatically use the host's map."
      : "Choose a battlefield size. Your configured units, supply, ranges, and fog rules stay unchanged.";
  renderMapOptions();
  showGameMenu("maps");
}

function handleMapSelection(event) {
  const card = event.target.closest("[data-map-id]");
  if (!card) return;
  selectedMapId = getMapPreset(card.dataset.mapId).id;
  renderMapOptions();
}

function updateLoadingProgress(percent, stage) {
  const progress = Math.max(0, Math.min(100, Math.round(percent)));
  els.loadingProgress.setAttribute("aria-valuenow", String(progress));
  els.loadingProgressBar.style.width = `${progress}%`;
  els.loadingStage.textContent = stage;
  els.loadingPercent.textContent = `${progress}%`;
}

function yieldLoadingFrame() {
  return new Promise((resolve) =>
    window.requestAnimationFrame(() => window.setTimeout(resolve, 35)),
  );
}

function showLoadingScreen(map) {
  els.loadingMapName.textContent = `${map.name} · ${map.cols} × ${map.rows} · ${(map.cols * map.rows).toLocaleString()} TILES`;
  updateLoadingProgress(0, "INITIALIZING COMMAND SYSTEMS");
  showGameMenu("loading");
}

async function prepareBattlefieldAssets(mapId) {
  const map = getMapPreset(mapId);
  showLoadingScreen(map);
  await yieldLoadingFrame();

  updateLoadingProgress(18, "ALLOCATING VIRTUAL MAP INDEX");
  gameConfig = applyMapPreset(developerConfigStore.load(), mapId);
  activeTerrainSeed = null;
  applyRuntimeRules();
  await yieldLoadingFrame();

  updateLoadingProgress(42, "DEPLOYING STARTING FORCES");
  state = freshState();
  battlefieldView = null;
  await yieldLoadingFrame();

  updateLoadingProgress(62, "BUILDING COMMAND INTERFACE");
  syncConfigDependentUI();
  await yieldLoadingFrame();

  updateLoadingProgress(78, "STREAMING TACTICAL TILE WINDOW");
  initializeBattlefield3D();
  await yieldLoadingFrame();

  updateLoadingProgress(94, "FINALIZING FOG AND ASSETS");
  render();
  preparedMapId = map.id;
  await yieldLoadingFrame();
  updateLoadingProgress(100, "OPERATION READY");
  await yieldLoadingFrame();
}

async function confirmMapSelection() {
  if (!isPlayableMap(getMapPreset(selectedMapId))) return;
  els.mapConfirmButton.disabled = true;
  try {
    if (pendingGameMode === "ai") await startAiCampaign(selectedMapId);
    else if (pendingGameMode === "human") {
      localOwner = 0;
      campaignVersion += 1;
      await prepareBattlefieldAssets(selectedMapId);
      showHumanMenu();
    }
  } catch (error) {
    console.error("Battlefield preparation failed.", error);
    showMapMenu(pendingGameMode ?? "ai");
    els.selectedMapCopy.textContent =
      "BATTLEFIELD PREPARATION FAILED · CHECK THE CONSOLE";
  } finally {
    renderSelectedMap();
  }
}

function showHumanMenu() {
  clearJoinError();
  showGameMenu("human");
}

function showLobby(message = "Establishing secure command link…") {
  showGameMenu("lobby");
  els.lobbyCopy.textContent = message;
  updateLobbyStatus();
}

function enterBattlefield() {
  els.gameMenu.hidden = true;
  document.querySelector(".app-shell").inert = false;
  window.requestAnimationFrame(() => battlefield3D?.resize?.());
  render();
}

async function startAiCampaign(mapId = selectedMapId) {
  closeMatchSocket(true);
  gameMode = "ai";
  localOwner = 0;
  multiplayer = createEmptyMultiplayerState();
  campaignVersion += 1;
  els.resultModal.hidden = true;
  await prepareBattlefieldAssets(mapId);
  enterBattlefield();
}

function clearJoinError() {
  els.joinMatchError.hidden = true;
  els.joinMatchError.textContent = "";
}

function showConnectionError(message) {
  els.joinMatchError.textContent = message;
  els.joinMatchError.hidden = false;
  if (!els.gameMenu.hidden && els.lobbyPanel.hidden) setMenuView("human");
  els.lobbyCopy.textContent = message;
}

function createHumanMatch() {
  clearJoinError();
  localOwner = 0;
  gameMode = "human";
  campaignVersion += 1;
  if (preparedMapId !== selectedMapId) {
    gameConfig = applyMapPreset(developerConfigStore.load(), selectedMapId);
    applyRuntimeRules();
  }
  state = freshState();
  battlefieldView = null;
  syncConfigDependentUI();
  if (preparedMapId !== selectedMapId || !battlefield3D) {
    initializeBattlefield3D();
    preparedMapId = selectedMapId;
  }
  connectMatch({ type: "create-match", state: createNetworkSnapshot() });
  showLobby("Opening a new synchronized campaign…");
}

function joinHumanMatch(event) {
  event.preventDefault();
  clearJoinError();
  const code = normalizeMatchCode(els.joinMatchCode.value);
  if (code.length !== 6) {
    showConnectionError("Enter the complete six-character match code.");
    els.joinMatchCode.focus();
    return;
  }
  gameMode = "human";
  connectMatch({ type: "join-match", code });
  showLobby(`Contacting campaign ${code}…`);
}

function resumeHumanMatch(code, token) {
  gameMode = "human";
  connectMatch({ type: "resume-match", code, token });
  showLobby(`Restoring campaign ${code}…`);
}

function connectMatch(request, { reconnect = false } = {}) {
  closeMatchSocket(true);
  intentionalSocketClose = false;
  window.clearTimeout(reconnectTimer);
  multiplayer.pendingRequest = request;
  multiplayer.reconnecting = reconnect;
  multiplayer.connected = false;
  renderMultiplayerStatus();

  let socket;
  try {
    socket = new WebSocket(websocketAddress());
  } catch {
    showConnectionError("This browser could not open a WebSocket connection.");
    return;
  }
  multiplayer.socket = socket;
  socket.addEventListener("open", () => {
    if (socket !== multiplayer.socket) return;
    multiplayer.connected = true;
    sendSocketMessage(request);
    renderMultiplayerStatus();
  });
  socket.addEventListener("message", (event) => {
    if (socket !== multiplayer.socket) return;
    try {
      handleMatchMessage(JSON.parse(event.data));
    } catch (error) {
      console.warn("Ignored invalid multiplayer message.", error);
    }
  });
  socket.addEventListener("close", () => {
    if (socket !== multiplayer.socket) return;
    if (multiplayer.synchronizing) {
      campaignVersion += 1;
      multiplayer.synchronizing = false;
    }
    multiplayer.connected = false;
    multiplayer.socket = null;
    renderMultiplayerStatus();
    if (!intentionalSocketClose && multiplayer.code && multiplayer.token) {
      multiplayer.reconnecting = true;
      reconnectTimer = window.setTimeout(
        () =>
          connectMatch(
            {
              type: "resume-match",
              code: multiplayer.code,
              token: multiplayer.token,
            },
            { reconnect: true },
          ),
        1500,
      );
    } else if (!intentionalSocketClose && !multiplayer.token) {
      showConnectionError(
        "The command server is unavailable. Try again shortly.",
      );
    }
  });
  socket.addEventListener("error", () => {
    multiplayer.connected = false;
    renderMultiplayerStatus();
  });
}

function closeMatchSocket(intentional = false) {
  intentionalSocketClose = intentional;
  window.clearTimeout(reconnectTimer);
  window.clearTimeout(networkSyncTimer);
  if (multiplayer.socket) {
    multiplayer.socket.close(1000, "Client leaving");
    multiplayer.socket = null;
  }
}

function sendSocketMessage(payload) {
  if (multiplayer.socket?.readyState !== WebSocket.OPEN) return false;
  multiplayer.socket.send(JSON.stringify(payload));
  return true;
}

function handleMatchMessage(message) {
  switch (message.type) {
    case "session":
    case "sync":
      handleSessionMessage(message);
      break;
    case "state-update":
      if (applyNetworkSnapshot(message.state, { cancelAnimations: false })) {
        saveMatchReference(message.state);
        render();
      }
      break;
    case "ready-update":
      updateReadyState(message.ready, message.connected);
      render();
      break;
    case "match-status":
      updateReadyState(message.ready, message.connected);
      if (message.players) updatePeerNames(message.players);
      if (
        message.pendingAdvance &&
        !message.completed?.[localOwner] &&
        !multiplayer.synchronizing
      )
        handlePhaseAdvance(message.pendingAdvance);
      maybeEnterHumanGame();
      render();
      break;
    case "phase-advance":
      handlePhaseAdvance(message);
      break;
    case "checkpoint":
      handleCheckpoint(message);
      break;
    case "error":
      showConnectionError(message.message || "The match request was rejected.");
      if (!multiplayer.token) {
        closeMatchSocket(true);
        gameMode = "menu";
        showHumanMenu();
      }
      break;
    default:
      break;
  }
}

function handleSessionMessage(message) {
  if (!isValidNetworkSnapshot(message.state)) {
    showConnectionError("The server returned an invalid campaign checkpoint.");
    return;
  }
  gameMode = "human";
  localOwner = message.playerIndex;
  multiplayer.code = message.code;
  multiplayer.token = message.token;
  multiplayer.connected = true;
  multiplayer.sequence = Number(message.sequence) || 0;
  multiplayer.synchronizing = Boolean(
    message.pendingAdvance && message.completed?.[localOwner],
  );
  multiplayer.reconnecting = false;
  multiplayer.pendingRequest = null;
  updateReadyState(message.ready, message.connected);
  updatePeerNames(message.players || []);
  applyNetworkSnapshot(message.state);
  saveMatchReference(message.state);
  updateLobbyStatus();

  if (message.pendingAdvance && !message.completed?.[localOwner]) {
    handlePhaseAdvance(message.pendingAdvance);
  } else if (message.pendingAdvance) {
    multiplayer.synchronizing = true;
  }
  maybeEnterHumanGame();
  render();
}

function updateReadyState(ready = [false, false], connected = [false, false]) {
  multiplayer.localReady = Boolean(ready?.[localOwner]);
  multiplayer.remoteReady = Boolean(ready?.[enemyOwner()]);
  multiplayer.opponentConnected = Boolean(connected?.[enemyOwner()]);
  multiplayer.connected = Boolean(connected?.[localOwner]);
  updateLobbyStatus();
}

function updatePeerNames(players) {
  const peer = players?.[enemyOwner()];
  if (peer?.name) els.lobbyOpponentName.textContent = peer.name.toUpperCase();
}

function maybeEnterHumanGame() {
  if (
    isHumanGame() &&
    multiplayer.code &&
    multiplayer.connected &&
    multiplayer.opponentConnected
  )
    enterBattlefield();
}

function updateLobbyStatus() {
  if (!els.lobbyPanel) return;
  els.lobbyMatchCode.textContent = multiplayer.code || "------";
  els.lobbyPlayerRole.textContent =
    localOwner === 0 ? "HOST COMMAND" : "SECOND COMMAND";
  const peerOnline = multiplayer.opponentConnected;
  els.lobbyOpponent.classList.toggle("connected", peerOnline);
  els.lobbyOpponentStatus.textContent = peerOnline ? "CONNECTED" : "OFFLINE";
  if (!peerOnline && multiplayer.code) {
    els.lobbyOpponentName.textContent = "AWAITING PEER";
    els.lobbyCopy.textContent =
      "Share this code with the second commander. The campaign deploys when both peers are connected.";
  }
  els.lobbyConnectionStatus.textContent = multiplayer.connected
    ? "COMMAND LINK ONLINE"
    : multiplayer.reconnecting
      ? "RECONNECTING"
      : "CONNECTING";
}

async function copyCurrentMatchCode() {
  if (!multiplayer.code) return;
  try {
    await navigator.clipboard.writeText(multiplayer.code);
    els.copyMatchCode.textContent = "COPIED";
    window.setTimeout(() => {
      els.copyMatchCode.textContent = "COPY CODE";
    }, 1400);
  } catch {
    els.copyMatchCode.textContent = multiplayer.code;
  }
}

function leaveHumanMatch() {
  if (isHumanGame() && multiplayer.code && multiplayer.token) {
    sendPlanUpdate();
    saveMatchReference();
  }
  campaignVersion += 1;
  closeMatchSocket(true);
  gameMode = "menu";
  localOwner = 0;
  multiplayer = createEmptyMultiplayerState();
  els.resultModal.hidden = true;
  showHumanMenu();
  render();
}

function schedulePlanSync() {
  if (!isHumanGame() || multiplayer.localReady || multiplayer.synchronizing)
    return;
  window.clearTimeout(networkSyncTimer);
  networkSyncTimer = window.setTimeout(sendPlanUpdate, 60);
}

function sendPlanUpdate() {
  window.clearTimeout(networkSyncTimer);
  if (!isHumanGame()) return;
  sendSocketMessage({
    type: "plan-update",
    jobs: state.jobs.filter((item) => item.owner === localOwner),
    orders: state.orders.filter((item) => item.owner === localOwner),
    resources: state.players[localOwner].resources,
  });
}

function setMultiplayerReady() {
  if (isInteractionLocked()) return;
  sendPlanUpdate();
  closeStructurePopover();
  state.selectedId = null;
  state.intent = null;
  state.buildMode = null;
  state.bridgeStart = null;
  multiplayer.localReady = true;
  sendSocketMessage({ type: "set-ready", ready: true });
  log("Command reported ready. Awaiting the opposing commander.");
  render();
}

async function handlePhaseAdvance(message) {
  const sequence = Number(message.sequence);
  if (
    !Number.isInteger(sequence) ||
    multiplayer.synchronizing ||
    !isValidNetworkSnapshot(message.state)
  )
    return;
  multiplayer.sequence = sequence;
  multiplayer.synchronizing = true;
  multiplayer.localReady = false;
  multiplayer.remoteReady = false;
  applyNetworkSnapshot(message.state);
  const activeCampaign = campaignVersion;
  render();

  const phase = state.phase;
  if (phase !== message.phase) {
    sendSocketMessage({ type: "request-sync" });
    return;
  }
  if (phase === "deployment") {
    state.phase = "orders";
    state.buildMode = null;
    state.bridgeStart = null;
    log("Both commanders are ready. Issue movement orders.");
    render();
  } else if (phase === "orders") {
    await phaseController.beginAttackPhase({ includeAi: false });
  } else if (phase === "attack") {
    resolutionRandom = seededRandom(message.seed);
    await phaseController.resolveRound({
      includeAi: false,
      random: resolutionRandom,
    });
  }

  if (activeCampaign !== campaignVersion) return;

  const completedState = createNetworkSnapshot();
  sendSocketMessage({
    type: "phase-complete",
    sequence,
    hash: snapshotHash(completedState),
    state: completedState,
  });
  saveMatchReference(completedState);
  render();
}

function handleCheckpoint(message) {
  if (!applyNetworkSnapshot(message.state)) return;
  multiplayer.sequence = Number(message.sequence) || multiplayer.sequence;
  multiplayer.synchronizing = false;
  multiplayer.localReady = false;
  multiplayer.remoteReady = false;
  saveMatchReference(message.state);
  if (message.hashesMatch === false)
    log("State divergence corrected from the host checkpoint.", true);
  render();
}

function renderMultiplayerStatus() {
  const human = isHumanGame();
  els.multiplayerStatus.hidden = !human;
  els.phaseSyncStatus.hidden = !human;
  if (!human) return;

  els.matchCodeDisplay.textContent = multiplayer.code || "------";
  const disconnected = !multiplayer.connected || !multiplayer.opponentConnected;
  els.multiplayerStatus.classList.toggle("disconnected", disconnected);
  els.phaseSyncStatus.classList.toggle(
    "waiting",
    multiplayer.localReady || multiplayer.synchronizing || disconnected,
  );

  let connectionCopy = "CONNECTED · SYNCHRONIZED";
  if (!multiplayer.connected)
    connectionCopy = multiplayer.reconnecting
      ? "RECONNECTING TO COMMAND SERVER"
      : "COMMAND SERVER OFFLINE";
  else if (!multiplayer.opponentConnected)
    connectionCopy = "OPPONENT OFFLINE · GAME PAUSED";
  else if (multiplayer.synchronizing)
    connectionCopy = "RESOLVING SHARED CHECKPOINT";
  else if (multiplayer.localReady)
    connectionCopy = "READY · WAITING FOR OPPONENT";
  els.multiplayerStatusCopy.textContent = connectionCopy;

  const readyCount =
    Number(multiplayer.localReady) + Number(multiplayer.remoteReady);
  els.phaseReadyCount.textContent = `${readyCount} / 2 READY`;
  els.localReadyState.innerHTML = `<i aria-hidden="true"></i> YOU · ${multiplayer.localReady ? "READY" : "PLANNING"}`;
  els.peerReadyState.innerHTML = `<i aria-hidden="true"></i> OPPONENT · ${multiplayer.remoteReady ? "READY" : multiplayer.opponentConnected ? "PLANNING" : "OFFLINE"}`;
  els.localReadyState.classList.toggle("ready", multiplayer.localReady);
  els.peerReadyState.classList.toggle("ready", multiplayer.remoteReady);

  if (multiplayer.synchronizing)
    els.phaseSyncCopy.textContent = "RESOLVING AND VERIFYING BOTH STATES";
  else if (!multiplayer.connected)
    els.phaseSyncCopy.textContent = "RECONNECTING — CAMPAIGN PAUSED";
  else if (!multiplayer.opponentConnected)
    els.phaseSyncCopy.textContent = "WAITING FOR OPPONENT TO REJOIN";
  else if (multiplayer.localReady)
    els.phaseSyncCopy.textContent = "YOUR ORDERS ARE LOCKED";
  else els.phaseSyncCopy.textContent = "ISSUE ORDERS, THEN REPORT READY";
}

function closeStructurePopover() {
  selectedStructureId = null;
  structurePopoverAnchor = null;
}

function selectStructure(structure, pointer = null) {
  if (!structure) return;
  selectedStructureId = structure.id;
  state.selectedId = null;
  state.intent = null;
  if (Number.isFinite(pointer?.clientX) && Number.isFinite(pointer?.clientY)) {
    const battlefieldBounds = els.battlefield.getBoundingClientRect();
    structurePopoverAnchor = {
      x: pointer.clientX - battlefieldBounds.left,
      y: pointer.clientY - battlefieldBounds.top,
    };
  } else {
    structurePopoverAnchor = {
      x: els.battlefield.clientWidth / 2,
      y: els.battlefield.clientHeight / 2,
    };
  }
}

function structureDescription(structure) {
  if (structure.type === "barracks")
    return "Produces ground combat units and deploys them onto a clear nearby land cell after resolution.";
  if (structure.type === "airfield")
    return "Builds, launches, and recovers one bomber assigned to this airfield.";
  if (structure.type === "mine")
    return `Feeds the supply network with ◆ ${gameConfig.mineIncome} at the end of every round.`;
  if (structure.type === "bridge")
    return "Creates an operational crossing that allows ground units to traverse water cells.";
  return "Operational field structure.";
}

function structureRecruitmentMarkup(structure) {
  const recruitableTypes = Object.entries(UNIT_TYPES).filter(
    ([type, spec]) =>
      spec.recruitable !== false &&
      productionBuildingType(type) === structure.type,
  );
  if (!recruitableTypes.length) return "";
  const friendly = structure.owner === localOwner;
  const queuedCount = state.jobs.filter(
    (job) =>
      job.kind === "recruit" &&
      (job.producerId ?? job.barracksId) === structure.id,
  ).length;
  const controlsAvailable =
    friendly && state.phase === "deployment" && !isInteractionLocked();
  const buttons = recruitableTypes
    .map(([type, spec]) => {
      const producerReady = producerCanRecruit(structure, type, localOwner);
      const affordable = state.players[localOwner].resources >= spec.price;
      const disabled = !controlsAvailable || !producerReady || !affordable;
      const reason = !friendly
        ? "Enemy production is unavailable."
        : state.phase !== "deployment"
          ? "Recruitment is available during deployment."
          : isInteractionLocked()
            ? "Command controls are currently locked."
            : !producerReady
              ? "This producer is at capacity."
              : !affordable
                ? `Requires ◆ ${spec.price}.`
                : `Recruit ${spec.label} from this ${BUILDINGS[structure.type].label}.`;
      return `<button class="structure-recruit" type="button" data-structure-recruit="${type}" data-testid="structure-recruit-${type}" aria-label="Recruit ${spec.label} for ${spec.price} supply" title="${reason}"${disabled ? " disabled" : ""}><i>${spec.mark}</i><strong>${spec.label}</strong><span>◆ ${spec.price}</span></button>`;
    })
    .join("");
  const note = !friendly
    ? "Production controls are restricted to the owning commander."
    : state.phase !== "deployment"
      ? "Recruitment controls reopen during the next deployment phase."
      : structure.type === "airfield" &&
          !producerCanRecruit(structure, "plane", localOwner)
        ? "This airfield already has a bomber assigned or in production."
        : "Recruits enter the manifest now and deploy after round resolution.";
  return `<div class="structure-production"><div class="structure-production-heading"><strong>PRODUCTION</strong><span>${queuedCount ? `${queuedCount} QUEUED` : "READY NEXT ROUND"}</span></div><div class="structure-recruit-grid">${buttons}</div><p class="structure-production-note">${note}</p></div>`;
}

function renderStructurePopover() {
  const structure = getStructure(selectedStructureId);
  if (!structure || structure.hp <= 0 || !isPieceVisible(structure)) {
    closeStructurePopover();
    els.structurePopover.hidden = true;
    return;
  }
  const spec = BUILDINGS[structure.type];
  if (!spec) {
    closeStructurePopover();
    els.structurePopover.hidden = true;
    return;
  }
  const friendly = structure.owner === localOwner;
  const ownerName =
    state.players.find((player) => player.id === structure.owner)?.name ??
    (friendly ? "FRIENDLY COMMAND" : "HOSTILE COMMAND");
  const footprintSize = occupiedCoordinates(structure).length;
  const footprint =
    structure.type === "bridge" && Number.isFinite(structure.lengthMeters)
      ? `${structure.lengthMeters.toFixed(1)} M SPAN`
      : `${footprintSize} CELL${footprintSize === 1 ? "" : "S"}`;
  const coverage = structure.revealsFog
    ? `${spec.range} CELL RADIUS`
    : "NONE";
  els.structurePopover.className = `structure-popover${friendly ? "" : " enemy"}`;
  els.structurePopover.innerHTML = `<header class="structure-popover-header"><span class="structure-popover-mark" aria-hidden="true">${spec.mark}</span><span class="structure-popover-heading"><small>${friendly ? "FRIENDLY STRUCTURE" : "IDENTIFIED HOSTILE STRUCTURE"}</small><strong id="structure-popover-title">${spec.label}</strong></span><button class="structure-popover-close" type="button" data-close-structure aria-label="Close structure information">×</button></header><div class="structure-popover-body"><p class="structure-popover-description">${structureDescription(structure)}</p><div class="structure-stat-grid"><span class="structure-stat"><small>COMMAND</small><strong>${ownerName}</strong></span><span class="structure-stat"><small>POSITION</small><strong>${coord(structure.x, structure.y)}</strong></span><span class="structure-stat"><small>INTEGRITY</small><strong>${structure.hp} / ${spec.lives} HP</strong></span><span class="structure-stat"><small>FOOTPRINT</small><strong>${footprint}</strong></span><span class="structure-stat"><small>STATUS</small><strong>OPERATIONAL</strong></span><span class="structure-stat"><small>FIELD COVERAGE</small><strong>${coverage}</strong></span></div>${structureRecruitmentMarkup(structure)}</div>`;
  els.structurePopover.hidden = false;

  const anchor = structurePopoverAnchor ?? {
    x: els.battlefield.clientWidth / 2,
    y: els.battlefield.clientHeight / 2,
  };
  const margin = 10;
  const width = els.structurePopover.offsetWidth;
  const height = els.structurePopover.offsetHeight;
  let left = anchor.x + 14;
  if (left + width + margin > els.battlefield.clientWidth)
    left = anchor.x - width - 14;
  left = Math.max(
    margin,
    Math.min(left, els.battlefield.clientWidth - width - margin),
  );
  const top = Math.max(
    margin,
    Math.min(
      anchor.y - 18,
      els.battlefield.clientHeight - height - margin,
    ),
  );
  els.structurePopover.style.left = `${left}px`;
  els.structurePopover.style.top = `${top}px`;
}

function render() {
  const selected = getUnit(state.selectedId);
  battlefieldView = createBattlefieldViewIndex({
    state,
    runtimeMap,
    localOwner,
    fogEnabled: gameConfig.fogEnabled,
    unitTypes: UNIT_TYPES,
    buildings: BUILDINGS,
    selected,
    movementRangeFor,
    canUnitAttack,
    isValidBuildTile,
    isTerrainPassable: (x, y) =>
      Number.isFinite(groundMovementCostAt(x, y)),
  });
  renderHeader(battlefieldView);
  renderPhase();
  renderMap(battlefieldView, selected);
  renderSelected();
  renderQueue();
  renderLog();
  renderPurchases();
  renderStructurePopover();
  renderMultiplayerStatus();
  renderTacticalNavigation(battlefieldView);
}

function niceMapGridStep(span) {
  const desired = Math.max(1, span / 10);
  const magnitude = 10 ** Math.floor(Math.log10(desired));
  const normalized = desired / magnitude;
  const interval =
    normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10;
  return interval * magnitude;
}

function renderTacticalNavigation(view) {
  if (!els.tacticalMap || !els.unitQuickNav || !runtimeMap || !view) return;
  const friendlyUnits = state.units
    .filter((unit) => unit.owner === localOwner && UNIT_TYPES[unit.type])
    .sort((first, second) => {
      const firstCommand = first.type === "commander" ? 0 : 1;
      const secondCommand = second.type === "commander" ? 0 : 1;
      return (
        firstCommand - secondCommand ||
        UNIT_TYPES[first.type].label.localeCompare(
          UNIT_TYPES[second.type].label,
        ) ||
        String(first.id).localeCompare(String(second.id))
      );
    });
  els.navigationUnitCount.textContent = `${friendlyUnits.length} UNIT${friendlyUnits.length === 1 ? "" : "S"}`;
  els.unitQuickNav.innerHTML = friendlyUnits.length
    ? friendlyUnits
        .map((unit) => {
          const spec = UNIT_TYPES[unit.type];
          const order = view.orderByUnit.get(unit.id);
          const status = unit.movedThisRound
            ? unit.airborne
              ? "SORTIE"
              : "MOVED"
            : order
              ? UNIT_TYPES[unit.type]?.airUnit
                ? "BOMB RUN"
                : order.kind.toUpperCase()
              : "READY";
          return `<button class="unit-nav-row${unit.id === state.selectedId ? " active" : ""}" type="button" data-unit-nav="${unit.id}" aria-label="Focus ${spec.label} at ${coord(unit.x, unit.y)}"><span class="unit-nav-mark">${spec.mark}</span><span class="unit-nav-copy"><strong>${spec.label}</strong><small>${coord(unit.x, unit.y)} / ${status}</small></span><b class="unit-nav-health">${unit.hp}/${spec.lives}</b></button>`;
        })
        .join("")
    : '<p class="unit-quick-nav-empty">NO FRIENDLY UNITS AVAILABLE</p>';

  if (els.navigationHud?.classList.contains("collapsed")) return;
  const canvas = els.tacticalMap;
  const bounds = runtimeMap.bounds;
  const cssWidth = Math.max(1, Math.round(canvas.clientWidth));
  const cssHeight = Math.max(1, Math.round(canvas.clientHeight));
  if (cssWidth <= 1 || cssHeight <= 1) return;
  const pixelRatio = Math.min(window.devicePixelRatio || 1, 1.5);
  const renderWidth = Math.max(1, Math.round(cssWidth * pixelRatio));
  const renderHeight = Math.max(1, Math.round(cssHeight * pixelRatio));
  if (canvas.width !== renderWidth || canvas.height !== renderHeight) {
    canvas.width = renderWidth;
    canvas.height = renderHeight;
  }
  const context = canvas.getContext("2d");
  if (!context) return;
  context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
  context.clearRect(0, 0, cssWidth, cssHeight);
  context.fillStyle = "#0d120e";
  context.fillRect(0, 0, cssWidth, cssHeight);

  const mapWidth = Math.max(bounds.maxX - bounds.minX, 1);
  const mapHeight = Math.max(bounds.maxY - bounds.minY, 1);
  const padding = 7;
  const scale = Math.min(
    (cssWidth - padding * 2) / mapWidth,
    (cssHeight - padding * 2) / mapHeight,
  );
  const planeWidth = mapWidth * scale;
  const planeHeight = mapHeight * scale;
  const offsetX = (cssWidth - planeWidth) / 2;
  const offsetY = (cssHeight - planeHeight) / 2;
  const project = (point) => ({
    x: offsetX + (point.x - bounds.minX) * scale,
    y: offsetY + (point.y - bounds.minY) * scale,
  });
  navigationMapProjection = {
    offsetX,
    offsetY,
    width: planeWidth,
    height: planeHeight,
    scale,
    bounds,
  };

  context.save();
  context.beginPath();
  context.rect(offsetX, offsetY, planeWidth, planeHeight);
  context.clip();
  const terrainImage = battlefield3D?.getTerrainMapImage?.() ?? null;
  if (terrainImage) {
    context.imageSmoothingEnabled = true;
    context.imageSmoothingQuality = "high";
    context.drawImage(
      terrainImage,
      offsetX,
      offsetY,
      planeWidth,
      planeHeight,
    );
  } else {
    context.fillStyle = "#52604d";
    context.fillRect(offsetX, offsetY, planeWidth, planeHeight);
  }
  if (gameConfig.fogEnabled) {
    context.fillStyle = "rgba(13, 18, 14, 0.7)";
    context.fillRect(offsetX, offsetY, planeWidth, planeHeight);
    context.save();
    context.beginPath();
    for (const source of view.visibilitySources) {
      const center = project(source);
      const radius = Math.max(1, source.radius * scale);
      context.moveTo(center.x + radius, center.y);
      context.arc(
        center.x,
        center.y,
        radius,
        0,
        Math.PI * 2,
      );
    }
    context.clip();
    if (terrainImage)
      context.drawImage(
        terrainImage,
        offsetX,
        offsetY,
        planeWidth,
        planeHeight,
      );
    else {
      context.fillStyle = "#52604d";
      context.fillRect(offsetX, offsetY, planeWidth, planeHeight);
    }
    context.restore();
  }

  const gridStep = niceMapGridStep(Math.max(mapWidth, mapHeight));
  context.strokeStyle = "rgba(210, 199, 121, 0.14)";
  context.lineWidth = 1;
  context.beginPath();
  for (
    let x = Math.ceil(bounds.minX / gridStep) * gridStep;
    x <= bounds.maxX;
    x += gridStep
  ) {
    const projected = project({ x, y: bounds.minY });
    context.moveTo(projected.x, offsetY);
    context.lineTo(projected.x, offsetY + planeHeight);
  }
  for (
    let y = Math.ceil(bounds.minY / gridStep) * gridStep;
    y <= bounds.maxY;
    y += gridStep
  ) {
    const projected = project({ x: bounds.minX, y });
    context.moveTo(offsetX, projected.y);
    context.lineTo(offsetX + planeWidth, projected.y);
  }
  context.stroke();
  context.restore();

  context.strokeStyle = "rgba(210, 199, 121, 0.38)";
  context.strokeRect(
    offsetX + 0.5,
    offsetY + 0.5,
    Math.max(0, planeWidth - 1),
    Math.max(0, planeHeight - 1),
  );
  navigationMarkerHits = [];
  const mapPieces = [
    ...state.structures,
    ...state.units,
    ...state.jobs.filter((job) => job.kind === "building"),
  ].filter((piece) => isPieceVisible(piece, view));
  for (const piece of mapPieces) {
    const center =
      runtimeMap.tileByCoordinate.get(`${piece.x},${piece.y}`)?.center ?? piece;
    let point = project(center);
    const friendly = piece.owner === localOwner;
    const isUnit = piece.type in UNIT_TYPES;
    const selected =
      piece.id === state.selectedId || piece.id === selectedStructureId;
    if (
      piece.type === "bridge" &&
      Number.isFinite(piece.endX) &&
      Number.isFinite(piece.endY)
    ) {
      const endCenter =
        runtimeMap.tileByCoordinate.get(`${piece.endX},${piece.endY}`)?.center ??
        { x: piece.endX, y: piece.endY };
      const endPoint = project(endCenter);
      context.beginPath();
      context.moveTo(point.x, point.y);
      context.lineTo(endPoint.x, endPoint.y);
      context.strokeStyle = friendly ? "#80b9b0" : "#d06f60";
      context.lineWidth = 2.5;
      context.stroke();
      point = {
        x: (point.x + endPoint.x) / 2,
        y: (point.y + endPoint.y) / 2,
      };
    }
    const radius = selected ? 4.5 : isUnit ? 3.25 : 2.75;
    context.beginPath();
    if (isUnit) context.arc(point.x, point.y, radius, 0, Math.PI * 2);
    else
      context.rect(
        point.x - radius,
        point.y - radius,
        radius * 2,
        radius * 2,
      );
    context.fillStyle = friendly ? "#80b9b0" : "#d06f60";
    context.fill();
    context.strokeStyle = selected
      ? "#d5f05a"
      : "rgba(12, 17, 13, 0.9)";
    context.lineWidth = selected ? 2 : 1;
    context.stroke();
    if (friendly && isUnit)
      navigationMarkerHits.push({
        id: piece.id,
        x: point.x,
        y: point.y,
        radius: Math.max(7, radius + 3),
      });
  }
}

function navigateToUnit(id) {
  const unit = getUnit(id);
  if (!unit || unit.owner !== localOwner) return;
  closeStructurePopover();
  if (
    !isInteractionLocked() &&
    (state.phase === "orders" || state.phase === "attack")
  ) {
    state.selectedId = unit.id;
    state.intent = defaultSelectionIntent(unit);
    render();
  }
  battlefield3D?.focusTile(unit.x, unit.y, { distance: 14 });
}

function handleTacticalMapClick(event) {
  if (!navigationMapProjection) return;
  const rect = els.tacticalMap.getBoundingClientRect();
  const pointer = {
    x: event.clientX - rect.left,
    y: event.clientY - rect.top,
  };
  const marker = navigationMarkerHits
    .filter(
      (item) =>
        Math.hypot(pointer.x - item.x, pointer.y - item.y) <= item.radius,
    )
    .sort(
      (first, second) =>
        Math.hypot(pointer.x - first.x, pointer.y - first.y) -
        Math.hypot(pointer.x - second.x, pointer.y - second.y),
    )[0];
  if (marker) {
    navigateToUnit(marker.id);
    return;
  }
  const projection = navigationMapProjection;
  if (
    pointer.x < projection.offsetX ||
    pointer.x > projection.offsetX + projection.width ||
    pointer.y < projection.offsetY ||
    pointer.y > projection.offsetY + projection.height
  )
    return;
  const mapPoint = {
    x:
      projection.bounds.minX +
      (pointer.x - projection.offsetX) / projection.scale,
    y:
      projection.bounds.minY +
      (pointer.y - projection.offsetY) / projection.scale,
  };
  let tile;
  if (runtimeMap.virtual) {
    tile = runtimeMap.getTile(Math.round(mapPoint.x), Math.round(mapPoint.y));
  } else {
    tile = runtimeMap.tiles.reduce((nearest, candidate) => {
      const candidateDistance = Math.hypot(
        candidate.center.x - mapPoint.x,
        candidate.center.y - mapPoint.y,
      );
      if (!nearest || candidateDistance < nearest.distance)
        return { tile: candidate, distance: candidateDistance };
      return nearest;
    }, null)?.tile;
  }
  if (tile) battlefield3D?.focusTile(tile.x, tile.y);
}

function renderHeader(view) {
  const visibleEnemies = [...state.units, ...state.structures].filter(
    (item) => item.owner === enemyOwner() && isPieceVisible(item, view),
  );
  const visibleUnits = visibleEnemies.filter(
    (item) => item.type in UNIT_TYPES,
  ).length;
  const visibleBarracks = visibleEnemies.filter(
    (item) => item.type === "barracks",
  ).length;
  els.round.textContent = String(state.round).padStart(2, "0");
  els.resources.textContent = state.players[localOwner].resources;
  els.income.textContent = `+${incomeFor(localOwner)} / round`;
  els.enemyForce.textContent = visibleEnemies.length
    ? `${visibleUnits} UNIT${visibleUnits === 1 ? "" : "S"} · ${visibleBarracks} BARRACKS SIGHTED`
    : "NO VISIBLE CONTACTS";
}

function renderPhase() {
  const phases = {
    deployment: { index: 1, label: "DEPLOYMENT", button: "PROCEED TO ORDERS" },
    orders: { index: 2, label: "ORDERS", button: "EXECUTE MOVEMENT" },
    attack: { index: 3, label: "ATTACK", button: "RESOLVE COMBAT" },
  };
  const deployment = state.phase === "deployment";
  const config =
    state.resolutionStep === "combat"
      ? { index: 4, label: "RESOLVE", button: "RESOLVING COMBAT" }
      : phases[state.phase];
  els.deploymentControls.hidden = !deployment;
  els.orderControls.hidden = deployment || state.resolutionStep === "combat";
  els.phaseIndex.textContent = String(config.index).padStart(2, "0");
  els.phaseName.textContent = config.label;
  els.phaseRing.textContent = config.index;
  let buttonLabel = config.button;
  if (isHumanGame()) {
    if (multiplayer.synchronizing) buttonLabel = "SYNCHRONIZING STATES";
    else if (!multiplayer.connected) buttonLabel = "RECONNECTING";
    else if (!multiplayer.opponentConnected)
      buttonLabel = "WAITING FOR OPPONENT";
    else if (multiplayer.localReady) buttonLabel = "READY · ORDERS LOCKED";
    else buttonLabel = `READY · ${config.button}`;
  }
  els.phaseButtonLabel.textContent = buttonLabel;

  const activeIndex =
    state.resolutionStep === "combat"
      ? 3
      : { deployment: 0, orders: 1, attack: 2 }[state.phase];
  [
    els.trackDeployment,
    els.trackOrders,
    els.trackAttack,
    els.trackResolve,
  ].forEach((track, index) => {
    track.className =
      index < activeIndex ? "done" : index === activeIndex ? "active" : "";
  });

  const attacking = state.phase === "attack";
  els.orderSectionTitle.textContent = attacking
    ? "DESIGNATE TARGETS"
    : "ISSUE MOVEMENT ORDERS";
  els.orderSectionLimit.textContent = attacking
    ? "STATIONARY UNITS ONLY"
    : "ONE ROUTE PER UNIT";
  els.orderSectionHint.textContent = attacking
    ? "Ground units that moved cannot fire. Airborne bombers resolve interception, payload release, and return before normal fire."
    : "Movement costs supply. Ground routes advance one flag per round; bomber movement remains a complete single-turn run.";
  els.moveButton.hidden = attacking;
  els.attackButton.hidden = !attacking;
  els.phaseButton.disabled = isInteractionLocked();
  document.body.classList.toggle(
    "resolving",
    state.resolutionStep === "combat",
  );
}

function isVirtualizedBattlefield() {
  return runtimeMap.virtual && runtimeMap.tileCount > VIRTUAL_RENDER_THRESHOLD;
}

function initialRenderCenter() {
  const friendlyPieces = [...state.units, ...state.structures].filter(
    (piece) => piece.owner === localOwner,
  );
  if (!friendlyPieces.length)
    return {
      x: (runtimeMap.bounds.minX + runtimeMap.bounds.maxX) / 2,
      y: (runtimeMap.bounds.minY + runtimeMap.bounds.maxY) / 2,
    };
  const centers = friendlyPieces.map(
    (piece) =>
      runtimeMap.tileByCoordinate.get(`${piece.x},${piece.y}`)?.center ?? piece,
  );
  return {
    x: centers.reduce((sum, center) => sum + center.x, 0) / centers.length,
    y: centers.reduce((sum, center) => sum + center.y, 0) / centers.length,
  };
}

function renderTileDescriptors(view, maximumTiles = MAX_DETAILED_TILES) {
  const virtualized = isVirtualizedBattlefield();
  const bounds = virtualized
    ? ((battlefield3D?.hasFocusedInitialView
        ? battlefield3D.getRenderBounds(maximumTiles)
        : null) ?? runtimeMap.windowAround(initialRenderCenter(), maximumTiles))
    : runtimeMap.bounds;
  const descriptors = runtimeMap.tilesInBounds(bounds, maximumTiles);
  if (!virtualized || !gameConfig.fogEnabled) return descriptors;
  const rangeOverlayAvailable = Boolean(battlefield3D);
  return descriptors.filter((tile) => {
    const key = `${tile.x},${tile.y}`;
    return (
      view.isVisible(tile.x, tile.y) ||
      (!rangeOverlayAvailable &&
        (view.available.has(key) ||
          view.reachable.has(key) ||
          view.targetable.has(key))) ||
      view.fireCountByTile.has(key)
    );
  });
}

function virtualRangeOverlay(selected) {
  if (!isVirtualizedBattlefield()) return { kind: null, sources: [] };
  const sourceFor = (piece, radius) => ({
    ...(runtimeMap.tileByCoordinate.get(`${piece.x},${piece.y}`)?.center ??
      piece),
    radius,
  });
  if (state.buildMode) {
    const buildSpec = BUILDINGS[state.buildMode];
    return {
      kind: "available",
      sources: [
        ...state.structures
          .filter(
            (piece) =>
              piece.owner === localOwner && !buildSpec?.commanderOnly,
          )
          .map((piece) =>
            sourceFor(piece, BUILDINGS[piece.type]?.buildRange ?? 2),
          ),
        ...state.units
          .filter(
            (piece) =>
              piece.owner === localOwner &&
              piece.type === "commander" &&
              (!state.bridgeStart?.commanderId ||
                piece.id === state.bridgeStart.commanderId),
          )
          .map((piece) => sourceFor(piece, movementRangeFor(piece))),
      ],
    };
  }
  if (selected && state.intent === "move")
    return {
      kind: "reachable",
      sources: [sourceFor(selected, movementRangeFor(selected))],
    };
  if (selected && state.intent === "attack")
    return {
      kind: "targetable",
      sources: [sourceFor(selected, UNIT_TYPES[selected.type].range)],
    };
  return { kind: null, sources: [] };
}

function renderMap(view, selected = getUnit(state.selectedId)) {
  const selectedStructure = getStructure(selectedStructureId);
  const renderFallbackGrid = !battlefield3D;
  const tiles = renderFallbackGrid ? [] : null;
  const fallbackBudget = isVirtualizedBattlefield()
    ? MAX_FALLBACK_TILES
    : runtimeMap.tileCount;
  const mapTiles = renderTileDescriptors(
    view,
    renderFallbackGrid ? fallbackBudget : MAX_DETAILED_TILES,
  );
  const tileModels = [];
  for (const mapTile of mapTiles) {
    const { x, y } = mapTile;
    const key = `${x},${y}`;
    const visible = view.isVisible(x, y);
    const isSelected = Boolean(
      (selected && selected.x === x && selected.y === y) ||
        (selectedStructure && occupiesTile(selectedStructure, x, y)),
    );
    // Nearby detailed instances retain per-tile interaction highlighting.
    // The strategic surface supplies the same range at distant LOD.
    const available = view.available.has(key);
    const reachable = view.reachable.has(key);
    const targetable = view.targetable.has(key);
    const concentratedFire = view.fireCountByTile.get(key) ?? 0;
    const terrain = terrainProperties(x, y, mapTile);
    const tileModel = {
      x,
      y,
      center: mapTile.center,
      vertices: mapTile.vertices,
      terrain: terrain.type,
      terrainType: terrain.terrainType,
      elevation: terrain.elevation,
      moisture: terrain.moisture,
      river: terrain.river,
      riverFlow: terrain.riverFlow,
      movementCost: terrain.movementCost,
      passable: terrain.passable,
      water: terrain.water,
      waterCoverage: terrain.waterCoverage,
      visible,
      selected: isSelected,
      available,
      reachable,
      targetable,
      fireCount: concentratedFire,
    };
    tileModels.push(tileModel);
    if (renderFallbackGrid) {
      const piece = view.occupancy.get(key) ?? null;
      const displayedPiece =
        piece && (piece.owner === localOwner || visible) ? piece : null;
      const showPieceModel =
        displayedPiece && displayedPiece.x === x && displayedPiece.y === y;
      const classes = ["tile", tileModel.terrain];
      if (!visible) classes.push("fogged");
      if (isSelected) classes.push("selected");
      if (available) classes.push("available");
      if (reachable) classes.push("reachable");
      if (targetable) classes.push("targetable");
      const label = tileAriaLabel(
        x,
        y,
        displayedPiece,
        visible,
        tileModel.water,
      );
      const gridPlacement = runtimeMap.virtual
        ? ` style="grid-column:${x + 1};grid-row:${y + 1}"`
        : "";
      tiles.push(`
        <button class="${classes.join(" ")}"${gridPlacement} type="button" role="gridcell" data-x="${x}" data-y="${y}" data-water="${tileModel.water}" data-testid="tile-${coord(x, y)}" aria-label="${label}${concentratedFire ? `, ${concentratedFire} fire order${concentratedFire === 1 ? "" : "s"} queued` : ""}">
          ${showPieceModel ? pieceMarkup(displayedPiece) : ""}
          ${concentratedFire ? `<span class="fire-stack-badge" data-testid="fire-stack-${coord(x, y)}">⌖ ×${concentratedFire}</span>` : ""}
          ${gameConfig.terrainDebugElevation ? `<span class="terrain-elevation-value">${terrain.elevation.toFixed(2)}</span>` : ""}
          <span class="tile-coordinate">${coord(x, y)}</span>
        </button>
        `);
    }
  }
  if (renderFallbackGrid) els.battlefieldGrid.innerHTML = tiles.join("");

  const pieces = [
    ...state.units,
    ...state.structures,
    ...state.jobs.filter((job) => job.kind === "building"),
    ...(state.buildMode === "bridge" && state.bridgeStart
      ? [
          {
            id: "bridge-start-guide",
            kind: "building",
            type: "bridge",
            owner: localOwner,
            x: state.bridgeStart.x,
            y: state.bridgeStart.y,
            hp: 1,
            guide: true,
            bridgeAnchorOnly: true,
          },
        ]
      : []),
  ]
    .filter((piece) => isPieceVisible(piece, view))
    .map((piece) => {
      const isUnit = piece.type in UNIT_TYPES;
      const spec = isUnit ? UNIT_TYPES[piece.type] : BUILDINGS[piece.type];
      const order =
        isUnit && piece.owner === localOwner && view.orderByUnit.get(piece.id);
      return {
        id: piece.id,
        kind: isUnit ? "unit" : "structure",
        type: piece.type,
        owner: piece.owner === localOwner ? 0 : 1,
        x: piece.x,
        y: piece.y,
        hp: piece.hp || 1,
        maxHp: spec.lives,
        pending: piece.kind === "building",
        guide: piece.guide === true,
        bridgeAnchorOnly: piece.bridgeAnchorOnly === true,
        selected:
          piece.id === state.selectedId || piece.id === selectedStructureId,
        orderKind: order?.kind || null,
        heading: piece.owner === 0 ? 0 : Math.PI,
        airborne: piece.airborne === true,
        footprintSize: isUnit ? 1 : (spec.footprintSize ?? 1),
        ...(piece.type === "bridge"
          ? {
              endX: piece.endX,
              endY: piece.endY,
              lengthMeters: piece.lengthMeters,
            }
          : {}),
      };
    });

  battlefield3D?.update({
    tiles: tileModels,
    pieces,
    orderTargets: state.orders
      .filter((order) => order.owner === localOwner && order.kind === "move")
      .filter((order) => {
        const actor = getUnit(order.unitId);
        return UNIT_TYPES[actor?.type]?.airUnit === true;
      })
      .map((order) => {
        const actor = getUnit(order.unitId);
        return {
          id: order.id,
          kind: UNIT_TYPES[actor?.type]?.airUnit === true ? "bomb" : "move",
          x: order.x,
          y: order.y,
        };
      }),
    movementRoutes: state.orders
      .filter((order) => order.owner === localOwner && order.kind === "move")
      .map((order) => {
        const actor = getUnit(order.unitId);
        if (!actor || UNIT_TYPES[actor.type]?.airUnit === true) return null;
        const waypointIndex = Number.isInteger(order.waypointIndex)
          ? order.waypointIndex
          : 0;
        const remainingWaypoints = Array.isArray(order.waypoints)
          ? order.waypoints.slice(waypointIndex)
          : [{ x: order.x, y: order.y }];
        const actorPathIndex = Array.isArray(order.path)
          ? order.path.findIndex(
              (point) => point.x === actor.x && point.y === actor.y,
            )
          : -1;
        const remainingPath = Array.isArray(order.path)
          ? order.path.slice(actorPathIndex >= 0 ? actorPathIndex + 1 : 0)
          : remainingWaypoints;
        return {
          id: order.id,
          points: [
            { x: actor.x, y: actor.y },
            ...remainingPath.map((point) => ({
              x: point.x,
              y: point.y,
            })),
          ],
          flagPoints: remainingWaypoints.map((waypoint) => ({
            x: waypoint.x,
            y: waypoint.y,
          })),
        };
      })
      .filter(Boolean),
    routePlanning:
      Boolean(selected) &&
      state.intent === "move" &&
      UNIT_TYPES[selected.type]?.airUnit !== true,
    fog: {
      enabled: isVirtualizedBattlefield() && gameConfig.fogEnabled,
      sources: view.visibilitySources,
    },
    rangeOverlay: virtualRangeOverlay(selected),
  });
}

function terrainProperties(x, y, mapTile = null) {
  if (mapTile?.terrain) {
    const terrainType = normalizeTerrainType(mapTile.terrain);
    const gameplay = terrainGameplayProperties(terrainType);
    return {
      x,
      y,
      elevation: Number.isFinite(mapTile.elevation) ? mapTile.elevation : 0.5,
      surfaceElevation: Number.isFinite(mapTile.surfaceElevation)
        ? mapTile.surfaceElevation
        : Number.isFinite(mapTile.elevation)
          ? mapTile.elevation
          : 0.5,
      moisture: Number.isFinite(mapTile.moisture) ? mapTile.moisture : 0.5,
      terrainType,
      type: terrainType,
      river: terrainType === "river",
      riverFlow: Number.isFinite(mapTile.riverFlow)
        ? mapTile.riverFlow
        : undefined,
      movementCost: gameplay.movementCost,
      passable: gameplay.passable,
      water: gameplay.water,
      waterCoverage: Number(gameplay.water),
    };
  }
  const cacheKey = `${x},${y}`;
  const cached = terrainTileCache.get(cacheKey);
  if (cached) return cached;
  const descriptor =
    mapTile ?? runtimeMap.tileByCoordinate.get(`${x},${y}`) ?? null;
  const center = descriptor?.center ?? { x, y };
  const terrainPoint = descriptor?.terrainPoint ?? center;
  const classified = classifyTerrainTile(
    terrainSampler,
    terrainPoint,
    descriptor?.vertices,
  );
  const terrain = { ...classified, x, y };
  // Streaming across a very large map must not turn the cache into a second
  // materialized copy of every tile in the theatre.
  if (terrainTileCache.size >= 50000)
    terrainTileCache.delete(terrainTileCache.keys().next().value);
  terrainTileCache.set(cacheKey, terrain);
  return terrain;
}

function terrainClass(x, y, mapTile = null) {
  return terrainProperties(x, y, mapTile).type;
}

function tileAriaLabel(x, y, piece, visible = true, water = false) {
  if (!visible) return `${coord(x, y)}, obscured by fog of war`;
  if (!piece) return `${coord(x, y)}, ${water ? "water" : "land"}, empty`;
  if (piece.kind === "building")
    return `${coord(x, y)}, planned ${BUILDINGS[piece.type].label}`;
  const owner = piece.owner === localOwner ? "friendly" : "hostile";
  const spec =
    piece.type in UNIT_TYPES ? UNIT_TYPES[piece.type] : BUILDINGS[piece.type];
  return `${coord(x, y)}, ${owner} ${spec.label}, ${piece.hp} health`;
}

function pieceMarkup(piece) {
  const pending = piece.kind === "building";
  const isUnit = piece.type in UNIT_TYPES;
  const spec = isUnit ? UNIT_TYPES[piece.type] : BUILDINGS[piece.type];
  const ownerClass = piece.owner === localOwner ? "player" : "enemy";
  const structureClass = isUnit ? "unit" : `structure ${piece.type}`;
  const order = isUnit && state.orders.find((item) => item.unitId === piece.id);
  const maxHp = pending ? 1 : spec.lives;
  const health = Array.from(
    { length: maxHp },
    (_, i) => `<i class="${i < (piece.hp || 1) ? "filled" : ""}"></i>`,
  ).join("");
  return `
    <span class="piece ${ownerClass} ${structureClass} ${pending ? "pending-piece" : ""}">
      <span class="piece-body"><span>${pending ? "+" : spec.mark}</span></span>
      ${pending ? "" : `<span class="health">${health}</span>`}
      ${order ? `<span class="order-badge">${order.kind === "move" ? "↗" : "⌖"}</span>` : ""}
    </span>
  `;
}

function renderSelected() {
  const selected = getUnit(state.selectedId);
  const selectedMovementRange = selected ? movementRangeFor(selected) : null;
  const movedThisRound = selected?.movedThisRound === true;
  const selectedIsAirUnit = UNIT_TYPES[selected?.type]?.airUnit === true;
  const currentOrderKind = state.phase === "attack" ? "attack" : "move";
  const orderCount = queuedOrderCount(selected, currentOrderKind);
  const orderLimit =
    currentOrderKind === "attack" ? attackActionLimitFor(selected) : 1;
  const hasOrder = Boolean(selected) && orderCount >= orderLimit;
  if (!selected) {
    els.selectedCard.className = "selected-card empty";
    els.selectedCard.innerHTML =
      '<span class="selected-unit-icon">—</span><span><strong>NO UNIT SELECTED</strong><small>Select a blue unit on the map.</small></span>';
    els.moveButton.disabled = true;
    els.attackButton.disabled = true;
    els.attackButton.title = "";
    els.attackRange.textContent = "Select unit";
  } else {
    const spec = UNIT_TYPES[selected.type];
    els.selectedCard.className = `selected-card${movedThisRound ? " moved" : ""}`;
    const actionLabel =
      state.phase === "attack" && orderLimit > 1
        ? `SHOTS ${orderCount}/${orderLimit}`
        : state.phase === "attack"
          ? "SHOT"
          : "MOVE";
    const movementStatus = selected?.airborne
      ? " · AIRBORNE · PAYLOAD ARMED"
      : movedThisRound
        ? " · REPOSITIONED · FIRE LOCKED"
        : "";
    const queuedStatus = orderCount ? ` · ${actionLabel} QUEUED` : "";
    const totalShots = spec.totalAttackActions;
    const ammunitionStatus = totalShots
      ? ` · AMMO ${Math.max(0, orderLimit - orderCount)}/${totalShots}`
      : "";
    els.selectedCard.innerHTML = `<span class="selected-unit-icon">${spec.mark}</span><span><strong>${spec.label} UNIT · ${coord(selected.x, selected.y)}</strong><small>${selected.hp}/${spec.lives} HP · ${damageFor(selected)} DMG · ◆ ${spec.actionCost} per ${selectedIsAirUnit ? "run" : "action"}${ammunitionStatus}${queuedStatus}${movementStatus}</small></span>`;
    els.moveButton.disabled =
      state.phase !== "orders" ||
      hasOrder ||
      state.players[localOwner].resources < spec.actionCost ||
      isInteractionLocked();
    els.attackButton.disabled =
      state.phase !== "attack" ||
      !canUnitAttack(selected) ||
      hasOrder ||
      state.players[localOwner].resources < spec.actionCost ||
      isInteractionLocked();
    els.attackRange.textContent = selectedIsAirUnit
      ? "Automatic payload"
      : `${spec.range} block range`;
    els.attackButton.title = selectedIsAirUnit
      ? "Bomber payloads resolve automatically from bombing-run orders."
      : movedThisRound
        ? "This unit moved during the command phase and cannot fire this round."
        : "";
  }
  const moveCopy = els.moveButton.querySelector("small");
  const moveLabel = els.moveButton.querySelector("strong");
  if (moveLabel) moveLabel.textContent = selectedIsAirUnit ? "BOMB RUN" : "MOVE";
  if (moveCopy)
    moveCopy.textContent = selected
      ? selectedIsAirUnit
        ? `Fly up to ${selectedMovementRange} blocks`
        : `${selectedMovementRange} blocks per round`
      : "Select a unit";
  els.moveButton.classList.toggle("active", state.intent === "move");
  els.attackButton.classList.toggle("active", state.intent === "attack");

  let message =
    state.phase === "deployment"
      ? "Click a Barracks or Airfield to recruit, or commission a new structure."
      : state.phase === "attack"
        ? "Select a friendly unit and designate a visible target square."
        : "Select a friendly unit and issue a movement order.";
  if (state.buildMode === "bridge")
    message = state.bridgeStart
      ? `Bridge start fixed at ${coord(state.bridgeStart.x, state.bridgeStart.y)}. Choose the opposite bank · ◆ ${BUILDINGS.bridge.pricePerMeter} per metre.`
      : "Choose a clear riverbank within your commander's movement radius to set the bridge start.";
  else if (state.buildMode)
    message = `Place the centre of ${BUILDINGS[state.buildMode].label}'s 3 × 3 footprint on a highlighted square.`;
  else if (selectedStructureId) {
    const selectedStructure = getStructure(selectedStructureId);
    if (selectedStructure)
      message = `${BUILDINGS[selectedStructure.type].label} selected at ${coord(selectedStructure.x, selectedStructure.y)}.`;
  }
  else if (selected?.airborne)
    message = `${UNIT_TYPES[selected.type].label} is over ${coord(selected.x, selected.y)} and will bomb, then return to its Airfield.`;
  else if (selected && state.intent === "move")
    message = selectedIsAirUnit
      ? `Choose the bombing-run target within ${selectedMovementRange} blocks of the Airfield.`
      : `Choose any connected destination. Flags will mark each round's ${selectedMovementRange}-block advance.`;
  else if (selected && state.intent === "attack")
    message = `Choose any square within ${UNIT_TYPES[selected.type].range} blocks to fire.`;
  else if (selected && state.phase === "attack" && movedThisRound)
    message = `${UNIT_TYPES[selected.type].label} repositioned this round and cannot fire.`;
  else if (selected)
    message = `${UNIT_TYPES[selected.type].label} unit selected at ${coord(selected.x, selected.y)}.`;
  els.selectionReadout.innerHTML = `<span class="selection-icon">⌖</span><span><small>FIELD ORDERS</small><strong>${message}</strong></span>`;
}

function updateBridgePlacementReadout(preview) {
  if (state.buildMode !== "bridge") return;
  const readout = els.selectionReadout.querySelector("strong");
  if (!readout) return;
  if (!state.bridgeStart) {
    readout.textContent = preview
      ? `${preview.previewValid ? "Set" : "Invalid"} bridge start at ${coord(preview.x, preview.y)}.`
      : "Choose a clear riverbank within your commander's movement radius to set the bridge start.";
    return;
  }
  if (!preview || !Number.isFinite(preview.endX)) {
    readout.textContent = `Bridge start fixed at ${coord(state.bridgeStart.x, state.bridgeStart.y)}. Choose the opposite bank · ◆ ${BUILDINGS.bridge.pricePerMeter} per metre.`;
    return;
  }
  const suffix = !preview.placementValid
    ? " · INVALID ROUTE"
    : !preview.affordable
      ? " · INSUFFICIENT SUPPLY"
      : " · CLICK TO BUILD";
  readout.textContent = `Bridge to ${coord(preview.endX, preview.endY)} · ${preview.lengthMeters.toFixed(1)} m · ◆ ${preview.cost}${suffix}`;
}

function renderQueue() {
  const playerJobs = state.jobs.filter((item) => item.owner === localOwner);
  const playerOrders = state.orders.filter((item) => item.owner === localOwner);
  const items = [...playerJobs, ...playerOrders];
  els.queueCount.textContent = `${items.length} QUEUED`;
  if (!items.length) {
    els.orderList.innerHTML =
      '<p class="empty-queue">No orders on the board.</p>';
    return;
  }

  els.orderList.innerHTML = items
    .map((item) => {
      if (item.kind === "building") {
        const detail =
          item.type === "bridge"
            ? `${coord(item.x, item.y)} to ${coord(item.endX, item.endY)} · ${Number(item.lengthMeters).toFixed(1)} m`
            : `Construction at ${coord(item.x, item.y)}`;
        return queueMarkup(
          item.id,
          "+",
          BUILDINGS[item.type].label,
          detail,
          item.cost,
          "build",
        );
      }
      if (item.kind === "recruit") {
        return queueMarkup(
          item.id,
          UNIT_TYPES[item.type].mark,
          `${UNIT_TYPES[item.type].label} RECRUIT`,
          "Reports after resolution",
          item.cost,
          "build",
        );
      }
      const actor = getUnit(item.unitId);
      const unitName = actor ? UNIT_TYPES[actor.type].label : "UNIT";
      const airMission = actor && UNIT_TYPES[actor.type]?.airUnit === true;
      const detail =
        item.kind === "move"
          ? airMission
            ? `Bombing run over ${coord(item.x, item.y)}`
            : `${Math.max(0, (item.waypoints?.length ?? 1) - (item.waypointIndex ?? 0))} turn route to ${coord(item.x, item.y)}`
          : `Fire on ${coord(item.x, item.y)}`;
      return queueMarkup(
        item.id,
        item.kind === "move" ? "↗" : "⌖",
        `${unitName} ORDER`,
        detail,
        item.cost,
        "action",
        item.kind !== "move" || (item.waypointIndex ?? 0) === 0,
        item.kind === "move" && !airMission && Array.isArray(item.waypoints),
      );
    })
    .join("");
}

function queueMarkup(
  id,
  icon,
  title,
  detail,
  cost,
  className,
  refundable = true,
  recurring = false,
) {
  const costLabel = recurring
    ? `◆ ${cost} / TURN`
    : `◆ ${cost}${refundable ? "" : " SPENT"}`;
  return `<div class="queue-item ${className}"><span class="queue-icon">${icon}</span><span><strong>${title}</strong><small>${detail}</small></span><span class="queue-cost">${costLabel}</span><button class="queue-remove" data-remove="${id}" type="button" aria-label="Cancel ${title}">×</button></div>`;
}

function renderLog() {
  els.combatLog.innerHTML = state.logs
    .slice(0, 3)
    .map(
      (entry) =>
        `<p class="${entry.danger ? "danger-log" : ""}">${entry.text}</p>`,
    )
    .join("");
}

function renderPurchases() {
  const deployment = state.phase === "deployment";
  const hasCommander = state.units.some(
    (item) =>
      item.owner === localOwner && item.type === "commander" && item.hp > 0,
  );
  document.querySelectorAll("[data-build]").forEach((button) => {
    const type = button.dataset.build;
    const spec = BUILDINGS[type];
    const minimumPrice = Number.isFinite(spec.price)
      ? spec.price
      : Math.max(1, Math.round(spec.pricePerMeter));
    button.disabled =
      !deployment ||
      isInteractionLocked() ||
      (spec.commanderOnly && !hasCommander) ||
      state.players[localOwner].resources < minimumPrice;
    button.classList.toggle("active", state.buildMode === type);
    button.title =
      spec.commanderOnly && !hasCommander
        ? "A surviving commander is required."
        : "";
  });
}

function chooseBuild(type) {
  const spec = BUILDINGS[type];
  const minimumPrice = Number.isFinite(spec?.price)
    ? spec.price
    : Math.max(1, Math.round(spec?.pricePerMeter ?? 0));
  const hasCommander = state.units.some(
    (item) =>
      item.owner === localOwner && item.type === "commander" && item.hp > 0,
  );
  if (
    !spec ||
    state.phase !== "deployment" ||
    isInteractionLocked() ||
    (spec.commanderOnly && !hasCommander) ||
    state.players[localOwner].resources < minimumPrice
  )
    return;
  state.buildMode = state.buildMode === type ? null : type;
  state.bridgeStart = null;
  state.selectedId = null;
  closeStructurePopover();
  render();
}

function commanderForBridgeStart(x, y, owner = localOwner) {
  if (
    terrainProperties(x, y).water ||
    piecesAt(x, y).length > 0 ||
    !navigationGraph.resolve({ x, y })
  )
    return null;
  return (
    state.units
      .filter(
        (item) =>
          item.owner === owner && item.type === "commander" && item.hp > 0,
      )
      .find(
        (commander) =>
          navigationGraph.isConnected(commander, { x, y }) &&
          distance(commander, { x, y }) <= movementRangeFor(commander),
      ) ?? null
  );
}

function isValidBridgeEnd(start, x, y, owner = localOwner) {
  if (!start || (start.x === x && start.y === y)) return false;
  const commander = state.units.find(
    (item) =>
      item.id === start.commanderId &&
      item.owner === owner &&
      item.type === "commander" &&
      item.hp > 0,
  );
  if (
    !commander ||
    terrainProperties(x, y).water ||
    piecesAt(x, y).length > 0 ||
    !navigationGraph.resolve({ x, y }) ||
    !navigationGraph.isConnected(commander, { x, y }) ||
    distance(commander, { x, y }) > movementRangeFor(commander)
  )
    return false;
  const { lengthMeters } = bridgeMetrics(start, { x, y });
  if (lengthMeters < 1) return false;
  const footprint = bridgeFootprintCoordinates(start, { x, y });
  const interior = footprint.slice(1, -1);
  return (
    isFootprintClear(footprint, { expectedSize: null }) &&
    interior.length > 0 &&
    interior.every((tile) => terrainProperties(tile.x, tile.y).water)
  );
}

function isValidBuildTile(
  x,
  y,
  owner = localOwner,
  type = owner === localOwner ? state.buildMode : null,
) {
  if (type === "bridge")
    return state.bridgeStart
      ? isValidBridgeEnd(state.bridgeStart, x, y, owner)
      : Boolean(commanderForBridgeStart(x, y, owner));
  const footprint = buildingFootprintCoordinates(x, y);
  if (
    !isFootprintClear(footprint) ||
    !footprint.every((tile) => {
      const terrain = terrainProperties(tile.x, tile.y);
      return terrain.passable && !terrain.water;
    })
  )
    return false;
  const ownStructures = state.structures.filter((item) => item.owner === owner);
  if (
    ownStructures.some(
      (item) =>
        navigationGraph.isConnected(item, { x, y }) &&
        distance(item, { x, y }) <=
          (BUILDINGS[item.type]?.buildRange ?? 2),
    )
  )
    return true;
  return state.units
    .filter((item) => item.owner === owner && item.type === "commander")
    .some(
      (commander) =>
        navigationGraph.isConnected(commander, { x, y }) &&
        distance(commander, { x, y }) <= movementRangeFor(commander),
    );
}

function queueBuilding(type, x, y, owner = localOwner) {
  const spec = BUILDINGS[type];
  if (!spec || spec.linear || !Number.isFinite(spec.price)) return false;
  const footprint = buildingFootprintCoordinates(x, y);
  if (
    (owner === localOwner &&
      (state.phase !== "deployment" || isInteractionLocked())) ||
    !isValidBuildTile(x, y, owner) ||
    state.players[owner].resources < spec.price
  )
    return false;
  state.players[owner].resources -= spec.price;
  state.jobs.push({
    id: uid("j"),
    kind: "building",
    type,
    owner,
    x,
    y,
    cost: spec.price,
    footprint,
  });
  if (owner === localOwner) {
    log(`${spec.label} construction marked at ${coord(x, y)}.`);
    schedulePlanSync();
  }
  return true;
}

function queueBridge(start, end, owner = localOwner) {
  const spec = BUILDINGS.bridge;
  const metrics = bridgeMetrics(start, end);
  const footprint = bridgeFootprintCoordinates(start, end);
  if (
    (owner === localOwner &&
      (state.phase !== "deployment" || isInteractionLocked())) ||
    !isValidBridgeEnd(start, end.x, end.y, owner) ||
    state.players[owner].resources < metrics.cost
  )
    return false;
  state.players[owner].resources -= metrics.cost;
  state.jobs.push({
    id: uid("j"),
    kind: "building",
    type: "bridge",
    owner,
    x: start.x,
    y: start.y,
    endX: end.x,
    endY: end.y,
    commanderId: start.commanderId,
    lengthMeters: metrics.lengthMeters,
    cost: metrics.cost,
    footprint,
  });
  if (owner === localOwner) {
    log(
      `${spec.label} construction marked from ${coord(start.x, start.y)} to ${coord(end.x, end.y)} · ${metrics.lengthMeters.toFixed(1)} m · ◆ ${metrics.cost}.`,
    );
    schedulePlanSync();
  }
  return true;
}

function queueRecruit(type, owner = localOwner, producerId = null) {
  const spec = UNIT_TYPES[type];
  const requestedProducer = producerId ? getStructure(producerId) : null;
  const producer = producerId
    ? requestedProducer
    : availableProducer(type, owner);
  if (
    !spec ||
    spec.recruitable === false ||
    !Number.isFinite(spec.price) ||
    (owner === localOwner &&
      (state.phase !== "deployment" || isInteractionLocked())) ||
    !producerCanRecruit(producer, type, owner) ||
    state.players[owner].resources < spec.price
  )
    return false;
  state.players[owner].resources -= spec.price;
  state.jobs.push({
    id: uid("j"),
    kind: "recruit",
    type,
    owner,
    producerId: producer.id,
    ...(producer.type === "barracks" ? { barracksId: producer.id } : {}),
    cost: spec.price,
  });
  if (owner === localOwner) {
    log(`${spec.label} unit added to the reinforcement manifest.`);
    schedulePlanSync();
  }
  render();
  return true;
}

function handleTileClick(x, y, pointer = null) {
  if (isInteractionLocked()) return;
  const clickedStructure = state.structures.find((structure) =>
    occupiesTile(structure, x, y),
  );
  if (state.phase === "deployment") {
    if (state.buildMode === "bridge") {
      closeStructurePopover();
      if (!state.bridgeStart) {
        const commander = commanderForBridgeStart(x, y, localOwner);
        if (commander) {
          state.bridgeStart = { x, y, commanderId: commander.id };
          log(`Bridge start fixed at ${coord(x, y)}. Choose the opposite bank.`);
        }
      } else if (
        state.bridgeStart.x === x &&
        state.bridgeStart.y === y
      ) {
        state.bridgeStart = null;
      } else if (queueBridge(state.bridgeStart, { x, y })) {
        state.buildMode = null;
        state.bridgeStart = null;
      } else if (isValidBridgeEnd(state.bridgeStart, x, y, localOwner)) {
        const { cost } = bridgeMetrics(state.bridgeStart, { x, y });
        if (state.players[localOwner].resources < cost)
          log(`Bridge requires ◆ ${cost}; insufficient supply.`, true);
      }
    } else if (state.buildMode) {
      closeStructurePopover();
      if (queueBuilding(state.buildMode, x, y)) state.buildMode = null;
    } else if (clickedStructure) {
      selectStructure(clickedStructure, pointer);
      render();
      return;
    } else closeStructurePopover();
    render();
    return;
  }

  if (state.phase !== "orders" && state.phase !== "attack") return;

  const clicked = pieceAt(x, y, false);
  const friendlyUnit = state.units.find(
    (unit) => unit.owner === localOwner && unit.x === x && unit.y === y,
  );
  if (friendlyUnit) {
    closeStructurePopover();
    state.selectedId = friendlyUnit.id;
    state.intent = defaultSelectionIntent(friendlyUnit);
    render();
    return;
  }

  const selected = getUnit(state.selectedId);
  if (clickedStructure && (!selected || clickedStructure.owner === localOwner)) {
    selectStructure(clickedStructure, pointer);
    render();
    return;
  }
  if (!selected) {
    if (clickedStructure) selectStructure(clickedStructure, pointer);
    else closeStructurePopover();
    render();
    return;
  }
  let orderQueued = false;
  if (state.intent === "move" && isValidMove(selected, x, y))
    orderQueued = queueOrder(selected, "move", x, y);
  else if (state.intent === "attack" && isValidTarget(selected, x, y))
    orderQueued = queueOrder(selected, "attack", x, y);
  else if (
    state.phase === "orders" &&
    UNIT_TYPES[selected.type]?.airUnit === true &&
    clicked?.owner === enemyOwner() &&
    isValidMove(selected, x, y)
  )
    orderQueued = queueOrder(selected, "move", x, y);
  else if (
    clicked &&
    clicked.owner === enemyOwner() &&
    isVisibleToPlayer(x, y) &&
    isValidTarget(selected, x, y)
  ) {
    state.intent = "attack";
    orderQueued = queueOrder(selected, "attack", x, y);
  } else if (clickedStructure) {
    selectStructure(clickedStructure, pointer);
    render();
    return;
  }
  if (orderQueued) closeStructurePopover();
  render();
}

function setIntent(intent) {
  const selected = getUnit(state.selectedId);
  const validForPhase =
    (intent === "move" && state.phase === "orders") ||
    (intent === "attack" && state.phase === "attack");
  if (
    !selected ||
    selected.owner !== localOwner ||
    isInteractionLocked() ||
    !validForPhase ||
    (intent === "attack" && !canUnitAttack(selected)) ||
    !canQueueUnitOrder(selected, intent)
  )
    return;
  state.intent = state.intent === intent ? null : intent;
  render();
}

function defaultSelectionIntent(unitItem) {
  const spec = UNIT_TYPES[unitItem?.type];
  return state.phase === "orders" &&
    spec?.airUnit === true &&
    unitItem.airborne !== true &&
    canQueueUnitOrder(unitItem, "move") &&
    state.players[localOwner].resources >= spec.actionCost
    ? "move"
    : null;
}

function isValidMove(actor, x, y, terrainPlan = undefined) {
  const isAirUnit = UNIT_TYPES[actor.type]?.airUnit === true;
  const homeAirfield = isAirUnit
    ? state.structures.find(
        (structure) =>
          structure.id === actor.homeAirfieldId &&
          structure.type === "airfield" &&
          structure.hp > 0,
      )
    : null;
  return (
    (!isAirUnit || (homeAirfield && actor.airborne !== true)) &&
    (isAirUnit || !isGroundOccupied(x, y)) &&
    (isAirUnit || Number.isFinite(groundMovementCostAt(x, y))) &&
    distance(actor, { x, y }) > 0 &&
    (!isAirUnit || distance(actor, { x, y }) <= movementRangeFor(actor)) &&
    navigationGraph.isConnected(actor, { x, y }) &&
    (isAirUnit ||
      Boolean(
        terrainPlan !== undefined
          ? terrainPlan
          : movementRoutePlan(actor, x, y),
      ))
  );
}

function isValidTarget(actor, x, y) {
  return (
    canUnitAttack(actor) &&
    distance(actor, { x, y }) > 0 &&
    distance(actor, { x, y }) <= UNIT_TYPES[actor.type].range
  );
}

function queueOrder(actor, kind, x, y, owner = localOwner) {
  const cost = UNIT_TYPES[actor.type].actionCost;
  const invalidPhase =
    (kind === "move" && state.phase !== "orders") ||
    (kind === "attack" && state.phase !== "attack");
  if (
    invalidPhase ||
    actor.owner !== owner ||
    (owner === localOwner && isInteractionLocked()) ||
    state.players[owner].resources < cost ||
    !canQueueUnitOrder(actor, kind)
  )
    return false;
  const isAirUnit = UNIT_TYPES[actor.type]?.airUnit === true;
  const movementPlan =
    kind === "move" && !isAirUnit
      ? movementRoutePlan(actor, x, y)
      : null;
  if (kind === "move" && !isValidMove(actor, x, y, movementPlan)) return false;
  if (kind === "attack" && !isValidTarget(actor, x, y)) return false;
  if (kind === "move" && !isAirUnit && !movementPlan?.waypoints.length)
    return false;
  state.players[owner].resources -= cost;
  state.orders.push({
    id: uid("o"),
    kind,
    owner,
    unitId: actor.id,
    x,
    y,
    cost,
    ...(movementPlan
      ? {
          path: movementPlan.path,
          waypoints: movementPlan.waypoints,
          waypointIndex: 0,
          paidWaypointIndex: 0,
          totalMovementCost: movementPlan.totalMovementCost,
        }
      : {}),
  });
  if (owner === localOwner) {
    log(
      isAirUnit && kind === "move"
        ? `${UNIT_TYPES[actor.type].label} bombing run designated over ${coord(x, y)}.`
        : kind === "move" && movementPlan.waypoints.length > 1
          ? `${UNIT_TYPES[actor.type].label} route plotted to ${coord(x, y)} across ${movementPlan.waypoints.length} turns at ◆ ${cost} per turn moved.`
          : `${UNIT_TYPES[actor.type].label} unit ordered to ${kind === "move" ? "advance to" : "fire on"} ${coord(x, y)}.`,
    );
    schedulePlanSync();
  }
  state.intent = null;
  return true;
}

function removeQueued(id) {
  const collection = state.jobs.some((item) => item.id === id)
    ? state.jobs
    : state.orders;
  const index = collection.findIndex(
    (item) => item.id === id && item.owner === localOwner,
  );
  if (index < 0 || isInteractionLocked()) return;
  const [item] = collection.splice(index, 1);
  const refundable =
    item.kind !== "move" ||
    !Number.isInteger(item.waypointIndex) ||
    item.waypointIndex === 0;
  if (refundable) state.players[localOwner].resources += item.cost;
  if (item.unitId === state.selectedId) state.intent = null;
  log(
    refundable
      ? "Order cancelled. Supply returned to stores."
      : "Route cancelled. Its committed supply has already been spent.",
  );
  schedulePlanSync();
  render();
}

function advancePhase() {
  if (state.resolving || state.winner) return;
  if (isHumanGame()) {
    setMultiplayerReady();
    return;
  }
  advancePhaseLocally();
}

function advancePhaseLocally() {
  if (state.phase === "deployment") {
    closeStructurePopover();
    state.phase = "orders";
    state.buildMode = null;
    state.bridgeStart = null;
    log("Deployment locked. Issue movement orders.");
    render();
    return;
  }
  if (state.phase === "orders") {
    phaseController.beginAttackPhase({ includeAi: true });
    return;
  }
  if (state.phase === "attack")
    phaseController.resolveRound({ includeAi: true });
}

function checkWinner() {
  const playerAlive =
    state.units.some((item) => item.owner === 0) ||
    playerBarracks(0).length > 0;
  const enemyAlive =
    state.units.some((item) => item.owner === 1) ||
    playerBarracks(1).length > 0;
  if (playerAlive && enemyAlive) return;
  state.winner = playerAlive ? 0 : enemyAlive ? 1 : "draw";
  state.resolving = false;
  showWinnerResult();
}

function showWinnerResult() {
  if (state.winner === localOwner) {
    els.resultTitle.textContent = "VICTORY";
    els.resultCopy.textContent = `The opposing command collapsed in round ${state.round}. The sector is secure.`;
  } else if (state.winner === enemyOwner()) {
    els.resultTitle.textContent = "DEFEAT";
    els.resultCopy.textContent = `Your command was overrun in round ${state.round}. Regroup and strike again.`;
  } else {
    els.resultTitle.textContent = "STALEMATE";
    els.resultCopy.textContent = "Both commands fell in the same exchange.";
  }
  els.resultModal.hidden = false;
}

function pause(ms) {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function initializeBattlefield3D() {
  battlefield3D?.destroy();
  battlefield3D = null;
  els.threeViewport.replaceChildren();
  document.body.classList.remove("three-enabled");
  try {
    const selected = getUnit(state.selectedId);
    const view = createBattlefieldViewIndex({
      state,
      runtimeMap,
      localOwner,
      fogEnabled: gameConfig.fogEnabled,
      unitTypes: UNIT_TYPES,
      buildings: BUILDINGS,
      selected,
      movementRangeFor,
      canUnitAttack,
      isValidBuildTile,
      isTerrainPassable: (x, y) =>
        Number.isFinite(groundMovementCostAt(x, y)),
    });
    const virtualized = isVirtualizedBattlefield();
    const initialTiles = renderTileDescriptors(view).map((tile) => {
      const terrain = terrainProperties(tile.x, tile.y, tile);
      return {
        ...tile,
        terrain: terrain.type,
        terrainType: terrain.terrainType,
        elevation: terrain.elevation,
        moisture: terrain.moisture,
        river: terrain.river,
        riverFlow: terrain.riverFlow,
        movementCost: terrain.movementCost,
        passable: terrain.passable,
        water: terrain.water,
      };
    });
    battlefield3D = new Battlefield3D(els.threeViewport, {
      cols: COLS,
      rows: ROWS,
      tiles: initialTiles,
      mapBounds: runtimeMap.bounds,
      virtualized,
      tileCapacity: MAX_DETAILED_TILES,
      renderChunkSize: runtimeMap.renderChunkSize,
      terrainSeed: activeTerrainSeed,
      terrain: terrainGenerator,
      terrainDebug: gameConfig.terrainDebug,
      showElevationValues: gameConfig.terrainDebugElevation,
      resolvePlacementPreview: (x, y) => {
        const type = state.buildMode;
        const spec = BUILDINGS[type];
        if (!type || !spec || state.phase !== "deployment") return null;
        if (type === "bridge") {
          if (!state.bridgeStart)
            return {
              id: "placement-preview",
              kind: "structure",
              type,
              owner: 0,
              x,
              y,
              hp: 1,
              maxHp: 1,
              pending: true,
              bridgeAnchorOnly: true,
              previewValid: Boolean(
                commanderForBridgeStart(x, y, localOwner),
              ),
              footprintSize: 1,
            };
          const metrics = bridgeMetrics(state.bridgeStart, { x, y });
          const placementValid = isValidBridgeEnd(
            state.bridgeStart,
            x,
            y,
            localOwner,
          );
          const affordable =
            state.players[localOwner].resources >= metrics.cost;
          return {
            id: "placement-preview",
            kind: "structure",
            type,
            owner: 0,
            x: state.bridgeStart.x,
            y: state.bridgeStart.y,
            endX: x,
            endY: y,
            hp: 1,
            maxHp: 1,
            pending: true,
            previewValid: placementValid && affordable,
            placementValid,
            affordable,
            lengthMeters: metrics.lengthMeters,
            cost: metrics.cost,
            footprintSize: 1,
          };
        }
        return {
          id: "placement-preview",
          kind: "structure",
          type,
          owner: 0,
          x,
          y,
          hp: 1,
          maxHp: 1,
          pending: true,
          previewValid: isValidBuildTile(x, y),
          footprintSize: spec.footprintSize ?? 1,
        };
      },
      onPlacementPreviewChange: updateBridgePlacementReadout,
      onTileClick: (x, y, pointer) => {
        if (isInteractionLocked()) return;
        handleTileClick(x, y, pointer);
      },
      onViewportChange: () => {
        if (!battlefield3D || !battlefieldView) return;
        renderMap(battlefieldView, getUnit(state.selectedId));
      },
    });
    document.body.classList.add("three-enabled");
    els.battlefieldGrid.replaceChildren();
    els.battlefieldGrid.hidden = true;
  } catch (error) {
    els.battlefieldGrid.hidden = false;
    console.warn(
      "3D battlefield unavailable; using the tactical grid fallback.",
      error,
    );
  }
}

createStaticUI();
renderSavedMatches();
showGameMenu("home");
render();
