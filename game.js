import { Battlefield3D } from "./battlefield3d.js";
import { MapBuilder3D } from "./map-builder3d.js";
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
  WORLD_CAMPAIGN_MAP_ID,
  allMapPresets,
  applyMapPreset,
  findMapPresetByDimensions,
  getMapPreset,
  registerCustomMaps,
} from "./maps/map-catalog.js";
import { createRuntimeMap, deploymentTiles } from "./maps/runtime-map.js";
import {
  classifyTerrainTile,
  createTerrainSampler,
  normalizeTerrainType,
  terrainGameplayProperties,
} from "./maps/procedural-terrain.js";
import { findTerrainPath } from "./terrain/terrain-pathfinding.js";
import { createChunkedTerrainSampler } from "./terrain/chunked-terrain.js";
import { hydrateTerrain } from "./terrain/terrain-generator.js";
import {
  cacheTerrain,
  getCachedTerrain,
} from "./terrain/terrain-cache.js";
import { loadPrebuiltTerrain } from "./terrain/terrain-binary.js";
import { createFreshState } from "./state/game-state.js";
import { createEntityIndex } from "./simulation/entity-index.js";
import {
  initializeRealtimeState,
  RtsRuntime,
  RTS_TICK_RATE,
  rtsStatsFor,
} from "./simulation/rts-runtime.js";
import {
  applySectorOutcome,
  canFactionContestSector,
  createWorldState,
} from "./simulation/world-state.js";
import { SavedMatchStore } from "./storage/saved-match-store.js";
import {
  deleteCustomMap,
  loadCustomMaps,
  saveCustomMap,
} from "./storage/custom-map-store.js";
import { createAuthoredTerrainData } from "./terrain/authored-terrain.js";
import { collectDomElements } from "./ui/dom-elements.js";
import { WorldGlobe } from "./world-globe.js";

let COLS = 48;
let ROWS = 48;
let navigationGraph = null;
let selectedMapId = DEFAULT_MAP_ID;
let activeTerrainSeed = null;
let runtimeMap = null;
let terrainSampler = null;
let terrainGenerator = null;
let terrainTileCache = new Map();
let activeTerrainModifications = [];
let pendingTerrainSeed = null;
let worldState = null;
let selectedWorldSectorId = null;
let activeWorldSectorId = null;
const customMapsReady = loadCustomMaps();
let customMapLibrary = [];

const CONFIG_STORAGE_KEY = "result-of-war:developer-config:v1";
const MATCH_STORAGE_KEY = "result-of-war:saved-matches:v1";
const UI_SETTINGS_STORAGE_KEY = "result-of-war:ui-settings:v1";
const WORLD_STORAGE_KEY = "result-of-war:world:v1";
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
let fastForwarding = false;
let settingsReturnView = "home";
let mapBuilderState = null;
let mapBuilder3D = null;
let mapBuilderMapId = null;
let worldGlobe = null;
let mapBuilderSculptMode = "add";
let multiplayer = createEmptyMultiplayerState();
const uid = (prefix) =>
  `${prefix}-${localOwner}-${Date.now().toString(36)}-${++idCounter}`;

function enemyOwner() {
  return localOwner === 0 ? 1 : 0;
}

function isHumanGame() {
  return gameMode === "human";
}

function isRealtimeGame() {
  return state?.phase === "realtime";
}

function isInteractionLocked() {
  return (
    fastForwarding ||
    state.resolving ||
    Boolean(state.winner) ||
    (isHumanGame() &&
      (!multiplayer.connected ||
        multiplayer.localReady ||
        multiplayer.synchronizing))
  );
}

function applyRuntimeRules({ terrain = null, resolutionScale = 1 } = {}) {
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
  if (terrain) {
    terrainGenerator = terrain;
    const baseSampler = (x, y) => terrain.sample(x, y);
    baseSampler.terrain = terrain;
    terrainSampler = createChunkedTerrainSampler(baseSampler, {
      modifications: activeTerrainModifications,
    });
  } else {
    const baseSampler = createTerrainSampler(
      runtimeMap.bounds,
      activeTerrainSeed,
      { ...preset.terrainConfig, resolutionScale },
    );
    terrainGenerator = baseSampler.terrain;
    terrainSampler = createChunkedTerrainSampler(baseSampler, {
      modifications: activeTerrainModifications,
    });
  }
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

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function freshState() {
  const deploymentFor = (owner) =>
    deploymentTiles(
      runtimeMap,
      owner,
      Math.min(runtimeMap.tileCount, Math.max(1024, ROWS * 6)),
    ).filter((tile) => terrainProperties(tile.x, tile.y, tile).passable);
  const nextState = createFreshState({
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
    objectiveCells: [0.25, 0.5, 0.75].flatMap((ratio) => {
      const target = {
        x: Math.floor(COLS / 2),
        y: Math.floor((ROWS - 1) * ratio),
      };
      for (let radius = 0; radius <= 12; radius += 1)
        for (let offsetY = -radius; offsetY <= radius; offsetY += 1)
          for (let offsetX = -radius; offsetX <= radius; offsetX += 1) {
            const x = target.x + offsetX;
            const y = target.y + offsetY;
            const terrain = terrainProperties(x, y);
            if (
              navigationGraph.resolve({ x, y }) &&
              terrain.passable &&
              !terrain.water
            )
              return [{ x, y }];
          }
      return [];
    }),
  });
  nextState.rtsIncomePerMinePerMinute = Math.max(
    6,
    gameConfig.mineIncome * 6,
  );
  return initializeRealtimeState(nextState, { tickRate: RTS_TICK_RATE });
}

let state = freshState();
let rtsRuntime = null;
let rtsAccumulator = 0;
let rtsPreviousFrame = 0;
let rtsLastRenderTick = -1;
let rtsLastFullRenderTick = -1;
let rtsAiNextTick = 0;
let rtsAiEconomyNextTick = 0;
let rtsPaused = false;
const rtsControlGroups = new Map();
let entityIndex = null;
let indexedUnits = null;
let indexedStructures = null;
let indexedUnitCount = -1;
let indexedStructureCount = -1;
let battlefield3D = null;
let battlefieldView = null;
let battlefieldViewTick = -1;
let battlefieldViewUiSignature = "";
let navigationMarkerHits = [];
let navigationMapProjection = null;
let navigationResizeFrame = null;
let navigationRenderSignature = "";
let navigationUiSignature = "";
let navigationLastRenderTime = 0;
const movementPathCache = new Map();
let movementPathObstacleSignature = "";
let selectedStructureId = null;
let structurePopoverAnchor = null;
const UNIT_COLLISION_RADIUS = 0.34;
const FORMATION_SPACING = 0.7;

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
  isGroundPositionOccupied,
  getBattlefield: () => battlefield3D,
  isVisible: isVisibleToPlayer,
  getLocalOwner: () => localOwner,
  getEnemyOwner: enemyOwner,
  coord,
  log,
  pause,
  modifyTerrain: (x, y, effect) => {
    if (!terrainSampler?.setModification) return;
    const current = terrainSampler(x, y);
    terrainSampler.setModification(x, y, {
      crater: Boolean(effect.crater),
      movementCost: Number.isFinite(current.movementCost)
        ? current.movementCost + (effect.movementCostDelta ?? 0)
        : current.movementCost,
    });
    terrainTileCache.delete(`${x},${y}`);
  },
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
  getWorldPoint: mapPositionFor,
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
  refreshMovementRoutes: refreshBlockedMovementRoutes,
  incomeFor,
  checkWinner,
  render,
  renderTransition: () => {
    renderPhase();
    renderQueue();
    renderLog();
    renderMultiplayerStatus();
  },
  log,
  pause,
});

function ensureRtsRuntime() {
  if (rtsRuntime?.state === state) return rtsRuntime;
  rtsRuntime = new RtsRuntime({
    state,
    unitTypes: UNIT_TYPES,
    buildingTypes: BUILDINGS,
    tickRate: RTS_TICK_RATE,
    seed: `${activeTerrainSeed}:${multiplayer.code || "local"}`,
    createId: uid,
    findPath: (unit, _start, destination) => {
      const tile = tileAtMapPoint(destination);
      if (!tile) return [];
      const plan = movementRoutePlan(
        unit,
        tile.x,
        tile.y,
        destination,
      );
      return plan?.path ?? [];
    },
    isPassable: (x, y) => {
      const tile = tileAtMapPoint({ x, y });
      return (
        tile &&
        !isStructureTileBlocked(tile.x, tile.y) &&
        Number.isFinite(groundMovementCostAt(tile.x, tile.y))
      );
    },
    isVisible: (owner, target) => isVisibleToOwner(owner, target),
    onEvent: handleRtsEvent,
  });
  return rtsRuntime;
}

function isVisibleToOwner(owner, target) {
  if (!gameConfig.fogEnabled) return true;
  return [...state.units, ...state.structures]
    .filter((piece) => piece.owner === owner && piece.revealsFog !== false)
    .some((piece) => {
      const spec = UNIT_TYPES[piece.type] ?? BUILDINGS[piece.type];
      return distance(piece, target) <= (spec?.visionRange ?? spec?.range ?? 0);
    });
}

function handleRtsEvent(event) {
  if (event.type === "production-complete") {
    if (!isHumanGame()) productionController.completeJob(event.job);
    return;
  }
  if (event.type === "weapon-fired") {
    const actor = getUnit(event.actorId);
    if (actor && battlefield3D)
      void battlefield3D.animateAttacks([
        {
          ...event,
          actorType: actor.type,
          owner: actor.owner === localOwner ? 0 : 1,
        },
      ]);
    return;
  }
  if (event.type === "entity-destroyed") {
    void battlefield3D?.animateDestructions?.([event.entityId]);
    if (event.owner === localOwner)
      log(`${UNIT_TYPES[event.entityType]?.label ?? BUILDINGS[event.entityType]?.label ?? "Asset"} lost at ${coord(event.x, event.y)}.`, true);
    return;
  }
  if (event.type === "aircraft-returning") {
    const aircraft = getUnit(event.unitId);
    if (aircraft?.owner === localOwner)
      log(`BOMBER diverting to the nearest Airfield with ${Math.max(0, Math.round(event.fuel))} fuel remaining.`);
    return;
  }
  if (event.type === "aircraft-landed") {
    const aircraft = getUnit(event.unitId);
    if (aircraft?.owner === localOwner)
      log("BOMBER landed and is refuelling.");
    return;
  }
  if (event.type === "aircraft-refuelled") {
    const aircraft = getUnit(event.unitId);
    if (aircraft?.owner === localOwner)
      log("BOMBER refuelled and awaiting orders.");
    return;
  }
  if (event.type === "aircraft-command-rejected") {
    const aircraft = getUnit(event.unitId);
    if (aircraft?.owner !== localOwner) return;
    log(
      event.reason === "insufficient-return-fuel"
        ? "Aircraft order rejected: the route does not preserve enough fuel to reach a friendly Airfield."
        : `Aircraft unavailable while ${event.reason}.`,
      true,
    );
  }
}

function issueRtsCommand(type, destination = null, targetId = null, {
  queued = false,
  units = selectedUnits(),
} = {}) {
  if (!units.length || isInteractionLocked()) return false;
  const accepted = ensureRtsRuntime().issueCommand({
    owner: localOwner,
    unitIds: units.map((unit) => unit.id),
    type,
    destination,
    targetId,
    queued,
  });
  if (!accepted.length) return false;
  state.intent = null;
  log(
    type === "attack"
      ? `${accepted.length} unit${accepted.length === 1 ? "" : "s"} ordered to focus fire.`
      : `${accepted.length} unit${accepted.length === 1 ? "" : "s"} executing ${type.replace("-", " ")}.`,
  );
  if (isHumanGame()) sendRealtimeCommand({ type, destination, targetId, unitIds: accepted, queued });
  render();
  return true;
}

function runRtsAi() {
  if (isHumanGame() || state.simulationTick < rtsAiNextTick) return;
  rtsAiNextTick = state.simulationTick + RTS_TICK_RATE * 3;
  if (state.simulationTick >= rtsAiEconomyNextTick) {
    rtsAiEconomyNextTick = state.simulationTick + RTS_TICK_RATE * 10;
    aiController.planDeploymentAndMoves({ includeMovement: false });
  }
  const runtime = ensureRtsRuntime();
  const enemies = state.units.filter((unit) => unit.owner === 1 && unit.hp > 0);
  const targets = [...state.units, ...state.structures].filter(
    (entity) =>
      entity.hp > 0 &&
      (entity.owner === 0 ||
        (entity.owner < 0 && BUILDINGS[entity.type]?.capturable)),
  );
  let issuedCommands = 0;
  for (const unit of enemies) {
    if (issuedCommands >= 2) break;
    if (runtime.commandFor(unit.id) || !targets.length) continue;
    let target = null;
    let targetDistance = Number.POSITIVE_INFINITY;
    for (const candidate of targets) {
      const candidateDistance = distance(unit, candidate);
      if (candidateDistance >= targetDistance) continue;
      target = candidate;
      targetDistance = candidateDistance;
    }
    if (!target) continue;
    const unitPosition = mapPositionFor(unit);
    const targetPosition = mapPositionFor(target);
    let destination = targetPosition;
    if (UNIT_TYPES[unit.type]?.airUnit !== true && targetDistance > 24) {
      const amount = 24 / targetDistance;
      const projected = {
        x: unitPosition.x + (targetPosition.x - unitPosition.x) * amount,
        y: unitPosition.y + (targetPosition.y - unitPosition.y) * amount,
      };
      const projectedTile = tileAtMapPoint(projected);
      const candidates = projectedTile
        ? runtimeMap.tilesWithinRadius(projectedTile, 5, {
            includeOrigin: true,
          })
        : [];
      let waypoint = null;
      let waypointDistance = Number.POSITIVE_INFINITY;
      for (const candidate of candidates) {
        if (
          !Number.isFinite(groundMovementCostAt(candidate.x, candidate.y)) ||
          isStructureTileBlocked(candidate.x, candidate.y)
        )
          continue;
        const candidateDistance = distance(candidate, target);
        if (candidateDistance >= waypointDistance) continue;
        waypoint = candidate;
        waypointDistance = candidateDistance;
      }
      if (!waypoint) continue;
      destination = waypoint.center ?? { x: waypoint.x, y: waypoint.y };
    }
    const accepted = runtime.issueCommand({
      owner: 1,
      unitIds: [unit.id],
      type: "attack-move",
      destination,
    });
    if (accepted.length) issuedCommands += 1;
  }
}

function rtsFrame(timestamp) {
  const previous = rtsPreviousFrame || timestamp;
  rtsPreviousFrame = timestamp;
  const active =
    gameMode !== "menu" &&
    els.gameMenu.hidden &&
    !rtsPaused &&
    !isHumanGame() &&
    !state.winner &&
    !(isHumanGame() && !multiplayer.connected);
  if (active) {
    rtsAccumulator += Math.min(0.2, Math.max(0, (timestamp - previous) / 1000));
    const tickDuration = 1 / RTS_TICK_RATE;
    let ticks = 0;
    while (rtsAccumulator >= tickDuration && ticks < 5) {
      ensureRtsRuntime().step();
      runRtsAi();
      rtsAccumulator -= tickDuration;
      ticks += 1;
    }
    if (
      ticks &&
      (state.simulationTick - rtsLastRenderTick >= 4 ||
        ensureRtsRuntime().events.length)
    ) {
      rtsLastRenderTick = state.simulationTick;
      ensureRtsRuntime().drainEvents();
      checkWinner();
      if (state.simulationTick - rtsLastFullRenderTick >= RTS_TICK_RATE) {
        rtsLastFullRenderTick = state.simulationTick;
        render();
      } else {
        renderMap(battlefieldView, getUnit(state.selectedId), {
          dynamicOnly: true,
        });
      }
    }
  } else {
    rtsAccumulator = 0;
  }
  requestAnimationFrame(rtsFrame);
}

function coord(x, y) {
  if (runtimeMap?.polygonal) {
    const index = runtimeMap.tileIndexByCoordinate.get(`${x},${y}`) ?? -1;
    return index >= 0 ? `T${String(index + 1).padStart(2, "0")}` : `${x},${y}`;
  }
  return `${columnLabel(x)}${y + 1}`;
}

function tileCenter(x, y) {
  return runtimeMap?.tileByCoordinate.get(`${x},${y}`)?.center ?? { x, y };
}

function mapPositionFor(piece) {
  if (!piece) return { x: 0, y: 0 };
  if (piece.center) return piece.center;
  if (Number.isFinite(piece.worldX) && Number.isFinite(piece.worldY))
    return { x: piece.worldX, y: piece.worldY };
  if (Number.isFinite(piece.x) && Number.isFinite(piece.y))
    return tileCenter(piece.x, piece.y);
  return { x: 0, y: 0 };
}

function targetPointForTile(x, y, pointer = null) {
  if (Number.isFinite(pointer?.worldX) && Number.isFinite(pointer?.worldY))
    return { x: pointer.worldX, y: pointer.worldY };
  return tileCenter(x, y);
}

function tileAtMapPoint(point) {
  if (!point || !Number.isFinite(point.x) || !Number.isFinite(point.y))
    return null;
  if (runtimeMap.virtual)
    return runtimeMap.getTile(Math.round(point.x), Math.round(point.y));
  return (
    runtimeMap.tiles.reduce((nearest, candidate) => {
      const candidateDistance = Math.hypot(
        candidate.center.x - point.x,
        candidate.center.y - point.y,
      );
      if (!nearest || candidateDistance < nearest.distance)
        return { tile: candidate, distance: candidateDistance };
      return nearest;
    }, null)?.tile ?? null
  );
}

function clearUnitSelection() {
  state.selectedId = null;
  state.selectedIds = [];
  state.intent = null;
}

function selectedUnits() {
  const ids = Array.isArray(state.selectedIds)
    ? state.selectedIds
    : state.selectedId
      ? [state.selectedId]
      : [];
  return ids
    .map((id) => getUnit(id))
    .filter((unit) => unit && unit.owner === localOwner && unit.hp > 0);
}

function selectedUnitIdSet() {
  return new Set(selectedUnits().map((unit) => unit.id));
}

function defaultSelectionIntentForUnits(units) {
  if (isRealtimeGame()) return units.length ? null : null;
  if (units.length > 1) {
    const groundGroup = units.every(
      (unit) => UNIT_TYPES[unit.type]?.airUnit !== true,
    );
    const totalMoveCost = units.reduce(
      (sum, unit) => sum + (UNIT_TYPES[unit.type]?.actionCost ?? 0),
      0,
    );
    return state.phase === "orders" &&
      groundGroup &&
      units.every((unit) => canQueueUnitOrder(unit, "move")) &&
      state.players[localOwner].resources >= totalMoveCost
      ? "move"
      : null;
  }
  return defaultSelectionIntent(units[0]);
}

function selectUnit(unit, { additive = false, pointer = null } = {}) {
  if (!unit || unit.owner !== localOwner) return;
  closeStructurePopover();
  selectedStructureId = null;
  const current = selectedUnits().map((item) => item.id);
  let nextIds;
  if (additive) {
    nextIds = current.includes(unit.id)
      ? current.filter((id) => id !== unit.id)
      : [...current, unit.id];
  } else {
    nextIds = [unit.id];
  }
  state.selectedIds = nextIds;
  state.selectedId = nextIds[0] ?? null;
  state.intent = defaultSelectionIntentForUnits(selectedUnits());
  if (
    unit.type === "commander" &&
    Number.isFinite(pointer?.clientX) &&
    Number.isFinite(pointer?.clientY)
  ) {
    const battlefieldBounds = els.battlefield.getBoundingClientRect();
    structurePopoverAnchor = {
      x: pointer.clientX - battlefieldBounds.left,
      y: pointer.clientY - battlefieldBounds.top,
    };
  } else if (unit.type !== "commander") structurePopoverAnchor = null;
}

function distance(a, b) {
  const first = mapPositionFor(a);
  const second = mapPositionFor(b);
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

function routePoint(point) {
  const tile = tileAtMapPoint(point);
  return tile
    ? { x: tile.x, y: tile.y, worldX: point.x, worldY: point.y }
    : null;
}

function sampleGroundSegment(start, end, { collect = false } = {}) {
  const length = euclideanDistance(start, end);
  const steps = Math.max(1, Math.ceil(length / 0.2));
  const samples = [];
  let cost = 0;
  let previous = start;
  for (let index = 1; index <= steps; index += 1) {
    const amount = index / steps;
    const point = {
      x: start.x + (end.x - start.x) * amount,
      y: start.y + (end.y - start.y) * amount,
    };
    const tile = tileAtMapPoint(point);
    if (!tile) return null;
    if (isStructureTileBlocked(tile.x, tile.y)) return null;
    const movementCost = groundMovementCostAt(tile.x, tile.y);
    if (!Number.isFinite(movementCost)) return null;
    const stepCost = euclideanDistance(previous, point) * movementCost;
    cost += stepCost;
    if (collect) samples.push({ point, tile, stepCost });
    previous = point;
  }
  return { cost, samples };
}

function smoothGroundPath(actor, gridPath, destinationPoint) {
  const start = mapPositionFor(actor);
  const candidates = [
    start,
    ...gridPath.slice(1, -1).map((tile) => tile.center ?? tileCenter(tile.x, tile.y)),
    destinationPoint,
  ];
  const smoothed = [start];
  let anchor = 0;
  while (anchor < candidates.length - 1) {
    let next = candidates.length - 1;
    while (next > anchor + 1) {
      if (sampleGroundSegment(candidates[anchor], candidates[next])) break;
      next -= 1;
    }
    smoothed.push(candidates[next]);
    anchor = next;
  }
  return smoothed;
}

function turnWaypointsForRoute(points, movementRange) {
  const waypoints = [];
  let turnRoute = [points[0]];
  let turnCost = 0;
  let totalMovementCost = 0;
  for (let segment = 1; segment < points.length; segment += 1) {
    let previous = points[segment - 1];
    const sampled = sampleGroundSegment(previous, points[segment], {
      collect: true,
    });
    if (!sampled) return null;
    for (const sample of sampled.samples) {
      let stepStart = previous;
      let stepCost = sample.stepCost;
      while (turnCost + stepCost > movementRange + 1e-6) {
        const remaining = movementRange - turnCost;
        const ratio = stepCost > 0 ? remaining / stepCost : 1;
        const stop = {
          x: stepStart.x + (sample.point.x - stepStart.x) * ratio,
          y: stepStart.y + (sample.point.y - stepStart.y) * ratio,
        };
        const waypoint = routePoint(stop);
        if (waypoint) {
          turnRoute.push(stop);
          waypoint.via = turnRoute.slice(1).map(routePoint).filter(Boolean);
          waypoints.push(waypoint);
          turnRoute = [stop];
        }
        totalMovementCost += remaining;
        stepStart = stop;
        stepCost -= remaining;
        turnCost = 0;
      }
      turnCost += stepCost;
      totalMovementCost += stepCost;
      previous = sample.point;
    }
    const segmentEnd = points[segment];
    const routeEnd = turnRoute[turnRoute.length - 1];
    if (euclideanDistance(routeEnd, segmentEnd) > 1e-6)
      turnRoute.push(segmentEnd);
  }
  const final = routePoint(points[points.length - 1]);
  const last = waypoints[waypoints.length - 1];
  if (
    final &&
    (!last ||
      Math.abs(last.worldX - final.worldX) > 1e-6 ||
      Math.abs(last.worldY - final.worldY) > 1e-6)
  ) {
    final.via = turnRoute.slice(1).map(routePoint).filter(Boolean);
    waypoints.push(final);
  }
  return { waypoints, totalMovementCost };
}

function movementRoutePlan(actor, x, y, targetPoint = null) {
  const destination = navigationGraph.resolve({ x, y });
  const current = navigationGraph.resolve(actor);
  if (!destination || !current) return null;
  const destinationPoint = targetPoint ?? tileCenter(x, y);
  if (current.id === destination.id) {
    const localRoute = turnWaypointsForRoute(
      [mapPositionFor(actor), destinationPoint],
      movementRangeFor(actor),
    );
    if (!localRoute || localRoute.totalMovementCost <= 0) return null;
    return {
      path: [{ x, y, worldX: destinationPoint.x, worldY: destinationPoint.y }],
      waypoints: localRoute.waypoints,
      totalMovementCost: localRoute.totalMovementCost,
    };
  }
  const obstacleSignature = [
    ...state.structures.map(
      (structure) =>
        `${structure.id}:${structure.x}:${structure.y}:${structure.hp}`,
    ),
    ...state.jobs
      .filter((job) => job.kind === "building")
      .map((job) => `${job.id}:${job.x}:${job.y}`),
  ].join("|");
  if (obstacleSignature !== movementPathObstacleSignature) {
    movementPathObstacleSignature = obstacleSignature;
    movementPathCache.clear();
  }
  const cacheKey = `${current.id}>${destination.id}`;
  let result = movementPathCache.get(cacheKey);
  if (!result) {
    result = findTerrainPath({
      graph: navigationGraph,
      start: current,
      goal: destination,
      movementCostAt: (tile) =>
        isStructureTileBlocked(tile.x, tile.y, {
          x: actor.x,
          y: actor.y,
        })
          ? Number.POSITIVE_INFINITY
          : groundMovementCostAt(tile.x, tile.y),
      distance,
      maximumVisited: Math.max(10000, Math.min(runtimeMap.tileCount, 75000)),
    });
    if (result) {
      if (movementPathCache.size >= 512)
        movementPathCache.delete(movementPathCache.keys().next().value);
      movementPathCache.set(cacheKey, result);
    }
  }
  if (!result || result.path.length < 2) return null;

  const continuousPath = smoothGroundPath(actor, result.path, destinationPoint);
  const planned = turnWaypointsForRoute(
    continuousPath,
    movementRangeFor(actor),
  );
  if (!planned?.waypoints.length) return null;
  const path = continuousPath.slice(1).map(routePoint).filter(Boolean);
  return { path, ...planned };
}

function refreshBlockedMovementRoutes() {
  const cancelledOrderIds = new Set();
  for (const order of state.orders) {
    if (order.kind !== "move") continue;
    const actor = getUnit(order.unitId);
    if (!actor || UNIT_TYPES[actor.type]?.airUnit === true) continue;
    const waypointIndex = Number.isInteger(order.waypointIndex)
      ? order.waypointIndex
      : 0;
    const waypoint = order.waypoints?.[waypointIndex];
    if (!waypoint) continue;
    const nextPoint = {
      x: Number.isFinite(waypoint.worldX) ? waypoint.worldX : waypoint.x,
      y: Number.isFinite(waypoint.worldY) ? waypoint.worldY : waypoint.y,
    };
    const routeStillClear =
      !isStructureTileBlocked(waypoint.x, waypoint.y) &&
      Boolean(sampleGroundSegment(mapPositionFor(actor), nextPoint));
    if (routeStillClear) continue;
    const destinationPoint = {
      x: Number.isFinite(order.worldX) ? order.worldX : order.x,
      y: Number.isFinite(order.worldY) ? order.worldY : order.y,
    };
    const replacement = movementRoutePlan(
      actor,
      order.x,
      order.y,
      destinationPoint,
    );
    if (!replacement?.waypoints.length) {
      cancelledOrderIds.add(order.id);
      if ((order.waypointIndex ?? 0) === 0)
        state.players[order.owner].resources += order.cost;
      if (order.owner === localOwner)
        log(
          `${UNIT_TYPES[actor.type].label} route cancelled because structures block every path.`,
          true,
        );
      continue;
    }
    order.path = replacement.path;
    order.waypoints = replacement.waypoints;
    order.waypointIndex = 0;
    order.paidWaypointIndex = 0;
  }
  if (cancelledOrderIds.size)
    state.orders = state.orders.filter(
      (order) => !cancelledOrderIds.has(order.id),
    );
}

function buildingFootprintTiles(x, y, type = null) {
  if (BUILDINGS[type]?.footprintSize === 1) {
    const tile = navigationGraph.resolve({ x, y });
    return tile ? [tile] : [];
  }
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

function buildingFootprintCoordinates(x, y, type = null) {
  return buildingFootprintTiles(x, y, type).map((tile) => ({
    x: tile.x,
    y: tile.y,
  }));
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
  const directionX = (endCenter.x - startCenter.x) / lengthMeters;
  const directionY = (endCenter.y - startCenter.y) / lengthMeters;
  const normal = { x: -directionY, y: directionX };
  if (normal.y < 0 || (Math.abs(normal.y) < 1e-6 && normal.x < 0)) {
    normal.x *= -1;
    normal.y *= -1;
  }
  for (const laneOffset of [0, BUILDINGS.bridge.width - 1.1]) {
    for (let index = 0; index <= sampleCount; index += 1) {
      const amount = index / sampleCount;
      const sample = {
        x:
          startCenter.x +
          (endCenter.x - startCenter.x) * amount +
          normal.x * laneOffset,
        y:
          startCenter.y +
          (endCenter.y - startCenter.y) * amount +
          normal.y * laneOffset,
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
  }
  return [...footprintByKey.values()].sort((first, second) => {
    const firstCenter =
      runtimeMap.tileByCoordinate.get(`${first.x},${first.y}`)?.center ?? first;
    const secondCenter =
      runtimeMap.tileByCoordinate.get(`${second.x},${second.y}`)?.center ??
      second;
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

function isStructureTileBlocked(x, y, ignoreOrigin = null) {
  if (ignoreOrigin?.x === x && ignoreOrigin?.y === y) return false;
  return (
    state.structures.some(
      (piece) =>
        piece.type !== "bridge" &&
        piece.hp > 0 &&
        occupiesTile(piece, x, y),
    ) ||
    state.jobs.some(
      (job) =>
        job.kind === "building" &&
        job.type !== "bridge" &&
        occupiesTile(job, x, y),
    )
  );
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

function isGroundPositionOccupied(
  x,
  y,
  worldX = null,
  worldY = null,
  ignoreUnitId = null,
  includePending = true,
) {
  const target = Number.isFinite(worldX) && Number.isFinite(worldY)
    ? { x: worldX, y: worldY }
    : tileCenter(x, y);
  const structureBlocks = state.structures.some(
    (piece) =>
      piece.type !== "bridge" &&
      piece.hp > 0 &&
      occupiesTile(piece, x, y),
  );
  if (structureBlocks) return true;
  if (
    includePending &&
    state.jobs.some(
      (job) =>
        job.kind === "building" &&
        job.type !== "bridge" &&
        occupiesTile(job, x, y),
    )
  )
    return true;

  return state.units.some((unit) => {
    if (unit.id === ignoreUnitId) return false;
    if (UNIT_TYPES[unit.type]?.airUnit === true && unit.airborne === true)
      return false;
    return euclideanDistance(mapPositionFor(unit), target) < UNIT_COLLISION_RADIUS;
  });
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

function getAttackTarget(x, y, actor, targetId = null) {
  const hostile = piecesAt(x, y, false).filter(
    (piece) =>
      piece.owner !== actor.owner &&
      piece.hp > 0 &&
      BUILDINGS[piece.type]?.capturable !== true,
  );
  const locked = targetId
    ? [...state.units, ...state.structures].find(
        (piece) =>
          piece.id === targetId &&
          piece.owner !== actor.owner &&
          piece.hp > 0 &&
          BUILDINGS[piece.type]?.capturable !== true,
      )
    : null;
  if (locked) return locked;
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
  return ensureEntityIndex().unit(id);
}

function getStructure(id) {
  return ensureEntityIndex().structure(id);
}

function ensureEntityIndex() {
  if (
    !entityIndex ||
    indexedUnits !== state.units ||
    indexedStructures !== state.structures ||
    indexedUnitCount !== state.units.length ||
    indexedStructureCount !== state.structures.length
  ) {
    entityIndex = createEntityIndex(state);
    indexedUnits = state.units;
    indexedStructures = state.structures;
    indexedUnitCount = state.units.length;
    indexedStructureCount = state.structures.length;
  }
  return entityIndex;
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
  return Number.isInteger(damage) && damage >= 0 ? damage : 1;
}

function canUnitAttack(unitItem) {
  const spec = UNIT_TYPES[unitItem?.type];
  return (
    Boolean(unitItem) &&
    spec?.automaticAttack !== true &&
    (spec?.damage ?? 0) > 0 &&
    (spec?.range ?? 0) > 0 &&
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
  const capacity = BUILDINGS[producer.type]?.aircraftCapacity ?? 1;
  const assigned = state.units.filter(
    (unit) => unit.homeAirfieldId === producer.id && unit.hp > 0,
  ).length;
  const queued = state.jobs.filter(
    (job) =>
      job.kind === "recruit" &&
      (job.producerId ?? job.barracksId) === producer.id &&
      UNIT_TYPES[job.type]?.airUnit === true,
  ).length;
  return assigned + queued < capacity;
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
    .filter((piece) => {
      if (piece.owner !== localOwner || piece.revealsFog !== true) return false;
      const spec = UNIT_TYPES[piece.type] ?? BUILDINGS[piece.type];
      return spec?.airUnit !== true || piece.airborne === true;
    })
    .some((piece) => {
      const spec = UNIT_TYPES[piece.type] ?? BUILDINGS[piece.type];
      const visionRange = spec.visionRange ?? spec.range ?? 0;
      return distance(piece, { x, y }) <= visionRange;
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
  worldGlobe = new WorldGlobe(els.worldSectorGrid, {
    onSelect: (sectorId) => {
      selectedWorldSectorId = sectorId;
      renderWorldCampaign();
    },
  });

  els.structurePopover.addEventListener("click", (event) => {
    const commanderBuild = event.target.closest("[data-commander-build]");
    if (commanderBuild) {
      chooseBuild(commanderBuild.dataset.commanderBuild);
      return;
    }
    const closeCommander = event.target.closest("[data-close-commander]");
    if (closeCommander) {
      clearUnitSelection();
      structurePopoverAnchor = null;
      render();
      return;
    }
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
    const commanderOpen =
      ["deployment", "realtime"].includes(state.phase) &&
      getUnit(state.selectedId)?.type === "commander";
    if (event.key !== "Escape" || (!selectedStructureId && !commanderOpen)) return;
    if (commanderOpen) clearUnitSelection();
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
      shiftKey: event.shiftKey,
    });
  });

  els.phaseButton.addEventListener("click", advancePhase);
  els.turnSkipButton?.addEventListener("click", fastForwardConfiguredTurns);
  els.moveButton.addEventListener("click", () => setIntent("move"));
  els.attackButton.addEventListener("click", () => setIntent("attack"));
  els.stopButton?.addEventListener("click", () =>
    issueRtsCommand("stop", null, null, { units: selectedUnits() }),
  );
  els.engageButton?.addEventListener("click", () =>
    issueRtsCommand("attack-stance", null, null, { units: selectedUnits() }),
  );
  els.fleeButton?.addEventListener("click", () =>
    issueRtsCommand("flee", null, null, { units: selectedUnits() }),
  );
  els.holdButton?.addEventListener("click", () =>
    issueRtsCommand("hold-fire", null, null, { units: selectedUnits() }),
  );
  els.orderList.addEventListener("click", (event) => {
    const button = event.target.closest("[data-remove]");
    if (button) removeQueued(button.dataset.remove);
  });
  els.unitQuickNav.addEventListener("click", (event) => {
    const button = event.target.closest("[data-unit-nav]");
    if (button) navigateToUnit(button.dataset.unitNav);
  });
  els.tacticalMap.addEventListener("click", handleTacticalMapClick);
  document.addEventListener("keydown", handleRtsKeyDown);
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
  els.resultMainMenu.addEventListener("click", leaveToMainMenu);
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
  els.mainSavesButton.addEventListener("click", () => showGameMenu("saves"));
  els.mainSettingsButton.addEventListener("click", () => openSettings("home"));
  els.mainMapBuilderButton.addEventListener("click", openMapBuilder);
  els.worldCampaignButton.addEventListener("click", openWorldCampaign);
  els.worldBackButton.addEventListener("click", showMainMenu);
  els.worldEnterSector.addEventListener("click", enterSelectedWorldSector);
  els.worldNewCampaign.addEventListener("click", resetWorldCampaign);
  els.mapBuilderBack.addEventListener("click", showMainMenu);
  els.mapBuilderSize.addEventListener("change", resetMapBuilder);
  els.mapBuilderClear.addEventListener("click", resetMapBuilder);
  els.mapBuilderNew.addEventListener("click", () =>
    resetMapBuilder({ newMap: true }),
  );
  els.mapBuilderLibraryList.addEventListener(
    "click",
    handleMapBuilderLibraryAction,
  );
  els.mapBuilderSave.addEventListener("click", saveMapBuilder);
  els.mapBuilderTool.addEventListener("change", updateMapBuilderToolControls);
  els.mapBuilderPickHeight.addEventListener("click", () => {
    mapBuilder3D?.beginHeightPick();
    els.mapBuilderPickHeight.classList.add("active");
    els.mapBuilderStatus.textContent =
      "Click a terrain point to use its height for flattening.";
  });
  for (const button of els.mapBuilderModeButtons)
    button.addEventListener("click", () => {
      mapBuilderSculptMode = button.dataset.mapBuilderMode;
      for (const modeButton of els.mapBuilderModeButtons)
        modeButton.classList.toggle(
          "active",
          modeButton === button,
        );
    });
  els.mapBuilderStrength.addEventListener("input", updateMapBuilderReadouts);
  els.mapBuilderSmoothing.addEventListener("input", updateMapBuilderReadouts);
  for (const input of [
    els.mapBuilderHeight,
    els.mapBuilderWater,
    els.mapBuilderLowColor,
    els.mapBuilderMidColor,
    els.mapBuilderHighColor,
    els.mapBuilderLowBoundary,
    els.mapBuilderHighBoundary,
  ])
    input.addEventListener("input", () => {
      updateMapBuilderReadouts();
      mapBuilder3D?.refreshColors();
    });
  els.savesBackButton.addEventListener("click", showMainMenu);
  els.settingsBackButton.addEventListener("click", () =>
    showGameMenu(settingsReturnView),
  );
  els.settingsForm.addEventListener("submit", applyMenuSettings);
  els.settingsOptionsTab.addEventListener("click", () =>
    showSettingsTab("options"),
  );
  els.settingsControlsTab.addEventListener("click", () =>
    showSettingsTab("controls"),
  );
  els.pauseResumeButton.addEventListener("click", enterBattlefield);
  els.pauseSaveButton.addEventListener("click", saveCurrentGame);
  els.pauseSettingsButton.addEventListener("click", () =>
    openSettings("pause"),
  );
  els.pauseLeaveButton.addEventListener("click", leaveToMainMenu);
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
  els.mainSavedMatchList.addEventListener("click", handleSavedMatchAction);
  els.copyMatchCode.addEventListener("click", copyCurrentMatchCode);
  els.cancelLobbyButton.addEventListener("click", leaveHumanMatch);
  els.leaveGameButton.addEventListener("click", leaveHumanMatch);
  els.menuButton.addEventListener("click", () => {
    showPauseMenu();
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

function handleRtsKeyDown(event) {
  if (
    !isRealtimeGame() ||
    gameMode === "menu" ||
    event.target.closest?.("input, select, textarea")
  )
    return;
  const key = event.key.toLowerCase();
  if (event.ctrlKey && /^[1-9]$/.test(key)) {
    rtsControlGroups.set(
      key,
      selectedUnits().map((unit) => unit.id),
    );
    event.preventDefault();
    return;
  }
  if (!event.ctrlKey && /^[1-9]$/.test(key)) {
    const ids = rtsControlGroups.get(key) ?? [];
    state.selectedIds = ids.filter((id) => getUnit(id)?.owner === localOwner);
    state.selectedId = state.selectedIds[0] ?? null;
    if (event.detail > 1 && state.selectedId)
      battlefield3D?.focusTile(getUnit(state.selectedId).x, getUnit(state.selectedId).y);
    render();
    return;
  }
  if (key === "a") setIntent("attack");
  else if (key === "m") setIntent("move");
  else if (key === "e")
    issueRtsCommand("attack-stance", null, null, { units: selectedUnits() });
  else if (key === "f")
    issueRtsCommand("flee", null, null, { units: selectedUnits() });
  else if (key === "s")
    issueRtsCommand("stop", null, null, { units: selectedUnits() });
  else if (key === "h")
    issueRtsCommand("hold-fire", null, null, { units: selectedUnits() });
  else if (key === "escape" && state.intent) {
    state.intent = null;
    state.buildMode = null;
    render();
  } else if (event.code === "Space" && !isHumanGame()) {
    rtsPaused = !rtsPaused;
    log(rtsPaused ? "Tactical pause engaged." : "Tactical pause released.");
    render();
    event.preventDefault();
  }
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
  } else if (
    event.key === "Escape" &&
    gameMode !== "menu" &&
    els.gameMenu.hidden &&
    state.buildMode
  ) {
    event.preventDefault();
    event.stopPropagation();
    cancelBuildPlacement();
  } else if (
    event.key === "Escape" &&
    gameMode !== "menu" &&
    els.helpModal.hidden &&
    els.resultModal.hidden
  ) {
    event.preventDefault();
    event.stopPropagation();
    if (els.gameMenu.hidden) showPauseMenu();
    else if (!els.menuPause.hidden) enterBattlefield();
    else if (!els.menuSettings.hidden && settingsReturnView === "pause")
      showGameMenu("pause");
  }
}

function cancelBuildPlacement() {
  if (!state.buildMode) return false;
  const label = BUILDINGS[state.buildMode]?.label ?? "construction";
  state.buildMode = null;
  state.bridgeStart = null;
  log(`${label} placement cancelled.`);
  render();
  return true;
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
  activeTerrainSeed = pendingTerrainSeed;
  pendingTerrainSeed = null;
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
  snapshot.terrain = { seed: activeTerrainSeed, mapId: selectedMapId };
  snapshot.terrain.modifications =
    terrainSampler?.serializeModifications?.() ?? [];
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
    ["deployment", "orders", "attack", "realtime"].includes(game.phase) &&
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
    selectedIds: Array.isArray(state.selectedIds) ? state.selectedIds : [],
    intent: state.intent,
    buildMode: state.buildMode,
    bridgeStart: state.bridgeStart,
    logs: state.logs,
  };
  gameConfig = { ...DEFAULT_GAME_CONFIG, ...snapshot.config };
  selectedMapId =
    snapshot.terrain?.mapId &&
    getMapPreset(snapshot.terrain.mapId).id === snapshot.terrain.mapId
      ? snapshot.terrain.mapId
      : findMapPresetByDimensions(gameConfig.cols, gameConfig.rows).id;
  activeTerrainSeed = snapshot.terrain?.seed ?? null;
  activeTerrainModifications = Array.isArray(snapshot.terrain?.modifications)
    ? cloneJson(snapshot.terrain.modifications)
    : [];
  applyRuntimeRules();
  const game = cloneJson(snapshot.game);
  for (const unit of game.units) {
    const maximum = Math.max(
      1,
      Number(UNIT_TYPES[unit.type]?.formationStrength) || 1,
    );
    if (!Number.isInteger(unit.maxStrength)) unit.maxStrength = maximum;
    if (!Number.isInteger(unit.strength)) unit.strength = unit.maxStrength;
  }
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
        structure.type,
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
      job.footprint = buildingFootprintCoordinates(job.x, job.y, job.type);
  }
  state = {
    ...game,
    resolving: false,
    resolutionStep: null,
    selectedIds: localUi.selectedIds.filter((selectedId) =>
      game.units.some(
        (unitItem) => unitItem.id === selectedId && unitItem.owner === localOwner,
      ),
    ),
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
  if (state.selectedId && !state.selectedIds.includes(state.selectedId))
    state.selectedIds.unshift(state.selectedId);
  if (!state.selectedId && state.selectedIds.length)
    state.selectedId = state.selectedIds[0];

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

function createLocalSaveToken() {
  const randomPart =
    globalThis.crypto?.randomUUID?.().replaceAll("-", "") ??
    Math.random().toString(36).slice(2).padEnd(24, "0");
  return `local_${Date.now().toString(36)}_${randomPart}`;
}

function saveCurrentGame() {
  if (gameMode === "menu") return;
  const snapshot = createNetworkSnapshot();
  if (isHumanGame() && multiplayer.code && multiplayer.token) {
    sendPlanUpdate();
    saveMatchReference(snapshot);
  } else {
    savedMatchStore.upsert({
      code: "SOLO01",
      token: createLocalSaveToken(),
      mode: "ai",
      mapId: selectedMapId,
      owner: localOwner,
      round: snapshot.game.round,
      phase: snapshot.game.phase,
      updatedAt: new Date().toISOString(),
      state: snapshot,
    });
    renderSavedMatches();
  }
  els.pauseMenuStatus.textContent = `CHECKPOINT SAVED · ROUND ${String(
    state.round,
  ).padStart(2, "0")}`;
}

function savedMatchMarkup(saved) {
  if (!saved.length)
    return '<p class="empty-saves">No saved operations on this device.</p>';
  return saved
    .map((item) => {
      const date = new Date(item.updatedAt);
      const dateLabel = Number.isNaN(date.valueOf())
        ? "LOCAL CHECKPOINT"
        : date.toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
      const solo = item.mode === "ai";
      const code = escapeHtml(item.code);
      const token = escapeHtml(item.token);
      const phase = escapeHtml(
        String(item.phase || "deployment").toUpperCase(),
      );
      return `<article class="saved-match-card"><span><strong>${solo ? "SOLO OPERATION" : code} · ROUND ${String(item.round || 1).padStart(2, "0")}</strong><small>${solo ? "FIELD COMMAND" : item.owner === 0 ? "HOST COMMAND" : "SECOND COMMAND"} · ${phase} · ${escapeHtml(dateLabel)}</small></span><button type="button" ${solo ? "data-resume-local" : `data-resume-code="${code}"`} data-resume-token="${token}">RESUME</button><button class="delete-save" type="button" data-delete-code="${code}" data-delete-token="${token}" aria-label="Remove saved operation">×</button></article>`;
    })
    .join("");
}

function renderSavedMatches() {
  const saved = savedMatchStore.load();
  els.savedMatchCount.textContent = `${saved.length} SAVED`;
  els.savedMatchList.innerHTML = savedMatchMarkup(
    saved.filter((item) => item.mode !== "ai"),
  );
  els.mainSavedMatchCount.textContent = `${saved.length} SAVED`;
  els.mainSavedMatchList.innerHTML = savedMatchMarkup(saved);
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
  els.menuSaves.hidden = view !== "saves";
  els.menuSettings.hidden = view !== "settings";
  els.menuPause.hidden = view !== "pause";
  els.menuMapBuilder.hidden = view !== "map-builder";
  els.menuWorld.hidden = view !== "world";
}

function loadWorldCampaign() {
  if (worldState) return worldState;
  try {
    const saved = JSON.parse(localStorage.getItem(WORLD_STORAGE_KEY));
    worldState =
      saved?.schema === 3 && Array.isArray(saved.sectors)
        ? saved
        : createWorldState();
  } catch {
    worldState = createWorldState();
  }
  return worldState;
}

function saveWorldCampaign() {
  try {
    localStorage.setItem(WORLD_STORAGE_KEY, JSON.stringify(worldState));
  } catch {
    // The active campaign remains playable when storage is unavailable.
  }
}

function openWorldCampaign() {
  loadWorldCampaign();
  selectedWorldSectorId ??=
    worldState.sectors.find((sector) =>
      canFactionContestSector(worldState, sector.id, localOwner),
    )?.id ??
    worldState.sectors[0]?.id;
  renderWorldCampaign();
  showGameMenu("world");
}

function resetWorldCampaign() {
  worldState = createWorldState({
    seed: globalThis.crypto?.randomUUID?.() ?? Date.now(),
  });
  selectedWorldSectorId =
    worldState.sectors.find((sector) =>
      canFactionContestSector(worldState, sector.id, localOwner),
    )?.id ?? null;
  saveWorldCampaign();
  renderWorldCampaign();
}

function renderWorldCampaign() {
  if (!worldState) return;
  const factions = new Map(
    worldState.factions.map((faction) => [faction.id, faction]),
  );
  const contestableIds = new Set(
    worldState.sectors
      .filter((sector) =>
        canFactionContestSector(worldState, sector.id, localOwner),
      )
      .map((sector) => sector.id),
  );
  worldGlobe?.setWorld(worldState, selectedWorldSectorId, contestableIds);
  const sector = worldState.sectors.find(
    (item) => item.id === selectedWorldSectorId,
  );
  const faction = sector ? factions.get(sector.owner) : null;
  els.worldGlobeCanvas.setAttribute(
    "aria-label",
    sector
      ? `Interactive campaign globe. Selected ${sector.name}, ${sector.biome}. Drag to rotate or use arrow keys to change sector.`
      : "Interactive strategic campaign globe.",
  );
  els.worldSectorBiome.textContent = sector
    ? sector.biome.toUpperCase()
    : "SELECT A SECTOR";
  els.worldSectorName.textContent = sector?.name ?? "THEATER OVERVIEW";
  els.worldSectorDescription.textContent = sector
    ? `A deterministic ${sector.biome} battlefield. Victory captures this sector and updates the persistent campaign.`
    : "Select a sector to review its strategic status.";
  els.worldSectorOwner.textContent = faction?.name ?? "UNCLAIMED";
  els.worldSectorObjective.textContent = (sector?.objective ?? "—")
    .replaceAll("-", " ")
    .toUpperCase();
  els.worldSectorSupply.textContent = sector
    ? sector.supply
      ? "CONNECTED"
      : "ISOLATED"
    : "—";
  const contestable = Boolean(sector && contestableIds.has(sector.id));
  els.worldEnterSector.disabled = !contestable;
  els.worldEnterSector.textContent = contestable
    ? "ENTER SECTOR"
    : sector?.owner === localOwner
      ? "SECTOR SECURED"
      : "FRONT OUT OF RANGE";
}

async function enterSelectedWorldSector() {
  const sector = worldState?.sectors.find(
    (item) => item.id === selectedWorldSectorId,
  );
  if (
    !sector ||
    !canFactionContestSector(worldState, sector.id, localOwner)
  )
    return;
  activeWorldSectorId = sector.id;
  pendingTerrainSeed = sector.terrainSeed;
  activeTerrainModifications = cloneJson(sector.modifications ?? []);
  await startAiCampaign(WORLD_CAMPAIGN_MAP_ID);
}

function showGameMenu(view = "home") {
  els.gameMenu.hidden = false;
  document.querySelector(".app-shell").inert = true;
  document.body.classList.toggle("world-campaign-active", view === "world");
  setMenuView(view);
  worldGlobe?.setActive(view === "world");
  if (view === "human" || view === "saves") renderSavedMatches();
}

function showMainMenu() {
  clearJoinError();
  pendingGameMode = null;
  showGameMenu("home");
}

function showPauseMenu() {
  if (gameMode === "menu") {
    showMainMenu();
    return;
  }
  els.pauseMenuStatus.textContent = "";
  showGameMenu("pause");
}

function leaveToMainMenu() {
  if (isHumanGame() && multiplayer.code && multiplayer.token) {
    sendPlanUpdate();
    saveMatchReference();
  }
  campaignVersion += 1;
  closeMatchSocket(true);
  gameMode = "menu";
  localOwner = 0;
  multiplayer = createEmptyMultiplayerState();
  activeWorldSectorId = null;
  activeTerrainModifications = [];
  els.resultModal.hidden = true;
  showMainMenu();
  render();
}

function readUiSettings() {
  try {
    return JSON.parse(localStorage.getItem(UI_SETTINGS_STORAGE_KEY)) ?? {};
  } catch {
    return {};
  }
}

function openSettings(returnView = "home") {
  settingsReturnView = returnView;
  const uiSettings = readUiSettings();
  els.settingsForm.elements.fogEnabled.checked = gameConfig.fogEnabled;
  els.settingsForm.elements.terrainDebug.checked = gameConfig.terrainDebug;
  els.settingsForm.elements.terrainDebugElevation.checked =
    gameConfig.terrainDebugElevation;
  els.settingsForm.elements.reducedMotion.checked =
    Boolean(uiSettings.reducedMotion);
  showSettingsTab("options");
  showGameMenu("settings");
}

function showSettingsTab(tab) {
  const controls = tab === "controls";
  els.settingsForm.hidden = controls;
  els.settingsControlsPanel.hidden = !controls;
  els.settingsOptionsTab.setAttribute("aria-selected", String(!controls));
  els.settingsControlsTab.setAttribute("aria-selected", String(controls));
}

function applyMenuSettings(event) {
  event.preventDefault();
  const form = els.settingsForm.elements;
  gameConfig = {
    ...gameConfig,
    fogEnabled: form.fogEnabled.checked,
    terrainDebug: form.terrainDebug.checked,
    terrainDebugElevation: form.terrainDebugElevation.checked,
  };
  developerConfigStore.save(gameConfig);
  const reducedMotion = form.reducedMotion.checked;
  try {
    localStorage.setItem(
      UI_SETTINGS_STORAGE_KEY,
      JSON.stringify({ reducedMotion }),
    );
  } catch {
    // The setting still applies for this session when storage is unavailable.
  }
  document.body.classList.toggle("reduced-motion", reducedMotion);
  if (gameMode !== "menu") {
    const viewState = battlefield3D?.getViewState?.();
    syncConfigDependentUI();
    initializeBattlefield3D({ viewState });
    render();
  }
  showGameMenu(settingsReturnView);
}

async function handleSavedMatchAction(event) {
  const deleteButton = event.target.closest("[data-delete-code]");
  if (deleteButton) {
    deleteSavedMatch(
      deleteButton.dataset.deleteCode,
      deleteButton.dataset.deleteToken,
    );
    return;
  }
  const button = event.target.closest("[data-resume-token]");
  if (!button) return;
  if (!button.hasAttribute("data-resume-local")) {
    resumeHumanMatch(button.dataset.resumeCode, button.dataset.resumeToken);
    return;
  }
  const saved = savedMatchStore
    .load()
    .find(
      (item) =>
        item.mode === "ai" && item.token === button.dataset.resumeToken,
    );
  if (!saved?.state) return;
  gameMode = "ai";
  localOwner = saved.owner ?? 0;
  multiplayer = createEmptyMultiplayerState();
  selectedMapId = saved.mapId ?? selectedMapId;
  const resumeMap = getMapPreset(selectedMapId);
  showLoadingScreen(resumeMap);
  updateLoadingProgress(12, "CHECKPOINT · OPENING SAVED OPERATION");
  await yieldLoadingFrame();
  updateLoadingProgress(34, "TERRAIN · RESTORING BATTLEFIELD SEED");
  await yieldLoadingFrame();
  if (!applyNetworkSnapshot(saved.state)) {
    gameMode = "menu";
    showGameMenu("saves");
    return;
  }
  updateLoadingProgress(82, "BATTLEFIELD · RESTORING UNITS AND ORDERS");
  await yieldLoadingFrame();
  syncConfigDependentUI();
  initializeBattlefield3D();
  render();
  updateLoadingProgress(100, "CHECKPOINT · OPERATION RESTORED");
  await yieldLoadingFrame();
  enterBattlefield();
}

function resetMapBuilder(options = {}) {
  const newMap = options?.newMap === true;
  if (newMap) {
    mapBuilderMapId = null;
    els.mapBuilderName.value = "CUSTOM FRONT";
  }
  const size = Number(els.mapBuilderSize.value) || 64;
  const elevation = new Float32Array(size * size);
  const trees = new Uint8Array(size * size);
  for (let y = 0; y < size; y += 1)
    for (let x = 0; x < size; x += 1) {
      const broad =
        Math.sin(x * 0.14) * 0.08 +
        Math.cos(y * 0.11) * 0.08 +
        Math.sin((x + y) * 0.055) * 0.09;
      elevation[y * size + x] = Math.max(0, Math.min(1, 0.52 + broad));
    }
  mapBuilderState = { size, elevation, trees };
  els.mapBuilderStatus.textContent = newMap
    ? "New map created. Drag on the terrain to begin."
    : "Terrain reset. Drag on the map to paint terrain.";
  mapBuilder3D?.setMap(mapBuilderState);
  renderMapBuilderLibrary();
}

function renderMapBuilderLibrary() {
  els.mapBuilderLibraryList.replaceChildren();
  if (!customMapLibrary.length) {
    const empty = document.createElement("p");
    empty.textContent = "NO SAVED MAPS";
    els.mapBuilderLibraryList.append(empty);
    return;
  }
  for (const map of customMapLibrary) {
    const row = document.createElement("article");
    row.className = map.id === mapBuilderMapId ? "active" : "";
    const details = document.createElement("span");
    const name = document.createElement("strong");
    name.textContent = map.name;
    const size = document.createElement("small");
    size.textContent = `${map.cols} × ${map.rows}`;
    details.append(name, size);
    const loadButton = document.createElement("button");
    loadButton.type = "button";
    loadButton.dataset.loadMap = map.id;
    loadButton.textContent = map.id === mapBuilderMapId ? "LOADED" : "LOAD";
    const deleteButton = document.createElement("button");
    deleteButton.type = "button";
    deleteButton.dataset.deleteMap = map.id;
    deleteButton.textContent = "DELETE";
    row.append(details, loadButton, deleteButton);
    els.mapBuilderLibraryList.append(row);
  }
}

function loadMapIntoBuilder(map) {
  const terrain = map?.authoredTerrain;
  if (
    !terrain?.elevation ||
    terrain.elevation.length !== map.cols * map.rows
  )
    return false;
  mapBuilderMapId = map.id;
  mapBuilderState = {
    size: map.cols,
    elevation: new Float32Array(terrain.elevation),
    trees: terrain.treeDensity
      ? new Uint8Array(terrain.treeDensity)
      : new Uint8Array(map.cols * map.rows),
  };
  els.mapBuilderName.value = map.name;
  els.mapBuilderSize.value = String(map.cols);
  els.mapBuilderWater.value = String(
    Math.round((terrain.waterLevel ?? 0.28) * 100),
  );
  const colors = terrain.heightColors ?? {};
  els.mapBuilderLowColor.value = colors.low ?? "#486f30";
  els.mapBuilderMidColor.value = colors.mid ?? "#7e8b45";
  els.mapBuilderHighColor.value = colors.high ?? "#b5ab7e";
  els.mapBuilderLowBoundary.value = String(
    Math.round((colors.lowBoundary ?? 0.45) * 100),
  );
  els.mapBuilderHighBoundary.value = String(
    Math.round((colors.highBoundary ?? 0.72) * 100),
  );
  mapBuilder3D?.setMap(mapBuilderState);
  updateMapBuilderReadouts();
  renderMapBuilderLibrary();
  els.mapBuilderStatus.textContent =
    `${map.name} loaded. Saving will update this map.`;
  return true;
}

async function handleMapBuilderLibraryAction(event) {
  const loadButton = event.target.closest("[data-load-map]");
  if (loadButton) {
    const map = customMapLibrary.find(
      (item) => item.id === loadButton.dataset.loadMap,
    );
    if (map) loadMapIntoBuilder(map);
    return;
  }
  const deleteButton = event.target.closest("[data-delete-map]");
  if (!deleteButton) return;
  const map = customMapLibrary.find(
    (item) => item.id === deleteButton.dataset.deleteMap,
  );
  if (!map || !window.confirm(`Delete ${map.name}? This cannot be undone.`))
    return;
  deleteButton.disabled = true;
  customMapLibrary = await deleteCustomMap(map.id);
  registerCustomMaps(customMapLibrary);
  if (selectedMapId === map.id) selectedMapId = DEFAULT_MAP_ID;
  if (mapBuilderMapId === map.id) resetMapBuilder({ newMap: true });
  renderMapBuilderLibrary();
  renderMapOptions();
  els.mapBuilderStatus.textContent = `${map.name} deleted.`;
}

function openMapBuilder() {
  if (!mapBuilder3D)
    mapBuilder3D = new MapBuilder3D(
      els.mapBuilderCanvas,
      () => ({
        tool: els.mapBuilderTool.value,
        mode: mapBuilderSculptMode,
        radius: Number(els.mapBuilderBrush.value) || 1,
        strength: Number(els.mapBuilderStrength.value) / 100,
        smoothing: Number(els.mapBuilderSmoothing.value) / 100,
        height: Number(els.mapBuilderHeight.value) / 100,
        water: Number(els.mapBuilderWater.value) / 100,
        lowBoundary: Number(els.mapBuilderLowBoundary.value) / 100,
        highBoundary: Number(els.mapBuilderHighBoundary.value) / 100,
        lowColor: els.mapBuilderLowColor.value,
        midColor: els.mapBuilderMidColor.value,
        highColor: els.mapBuilderHighColor.value,
      }),
      {
        onHeightPicked: (height) => {
          els.mapBuilderHeight.value = String(Math.round(height * 100));
          els.mapBuilderTool.value = "flatten";
          updateMapBuilderToolControls();
          els.mapBuilderPickHeight.classList.remove("active");
          els.mapBuilderStatus.textContent =
            `Flatten height sampled at ${height.toFixed(2)}. Drag to level terrain.`;
          updateMapBuilderReadouts();
        },
      },
    );
  if (!mapBuilderState) resetMapBuilder();
  else mapBuilder3D.setMap(mapBuilderState);
  renderMapBuilderLibrary();
  updateMapBuilderReadouts();
  updateMapBuilderToolControls();
  showGameMenu("map-builder");
  requestAnimationFrame(() => mapBuilder3D?.resize());
}

function updateMapBuilderReadouts() {
  els.mapBuilderStrengthValue.value =
    `${Math.round(Number(els.mapBuilderStrength.value))}%`;
  els.mapBuilderSmoothingValue.value =
    `${Math.round(Number(els.mapBuilderSmoothing.value))}%`;
  els.mapBuilderHeightValue.value = (
    Number(els.mapBuilderHeight.value) / 100
  ).toFixed(2);
  els.mapBuilderWaterValue.value = (
    Number(els.mapBuilderWater.value) / 100
  ).toFixed(2);
}

function updateMapBuilderToolControls() {
  const tool = els.mapBuilderTool.value;
  els.mapBuilderMode.hidden = tool !== "sculpt";
  els.mapBuilderStrengthControl.hidden = tool === "smooth";
  els.mapBuilderSmoothingControl.hidden = tool !== "smooth";
  els.mapBuilderHeightControl.hidden = tool !== "flatten";
  if (tool === "smooth")
    els.mapBuilderStatus.textContent =
      "Drag across terrain to soften sharp elevation changes.";
}

async function saveMapBuilder() {
  if (!mapBuilderState) return;
  els.mapBuilderSave.disabled = true;
  els.mapBuilderStatus.textContent = "Saving map terrain...";
  const waterLevel = Number(els.mapBuilderWater.value) / 100;
  let waterCount = 0;
  for (const value of mapBuilderState.elevation)
    if (value < waterLevel) waterCount += 1;
  if (waterCount / mapBuilderState.elevation.length > 0.3) {
    els.mapBuilderStatus.textContent =
      "Water exceeds the 30% map limit. Raise terrain or lower water height.";
    els.mapBuilderSave.disabled = false;
    return;
  }
  const id = mapBuilderMapId ?? `custom-${Date.now().toString(36)}`;
  const name = els.mapBuilderName.value.trim() || "CUSTOM FRONT";
  const map = {
    id,
    name: name.toUpperCase(),
    theater: "AUTHORED BATTLEFIELD",
    cols: mapBuilderState.size,
    rows: mapBuilderState.size,
    preview: "custom",
    description: "A locally authored terrain map.",
    authoredTerrain: {
      elevation: mapBuilderState.elevation,
      treeDensity: mapBuilderState.trees,
      waterLevel,
      heightColors: {
        low: els.mapBuilderLowColor.value,
        mid: els.mapBuilderMidColor.value,
        high: els.mapBuilderHighColor.value,
        lowBoundary: Number(els.mapBuilderLowBoundary.value) / 100,
        highBoundary: Number(els.mapBuilderHighBoundary.value) / 100,
      },
    },
  };
  try {
    customMapLibrary = await saveCustomMap(map);
    registerCustomMaps(customMapLibrary);
  } catch {
    els.mapBuilderStatus.textContent =
      "The browser could not save this map. Check available device storage.";
    els.mapBuilderSave.disabled = false;
    return;
  }
  mapBuilderMapId = id;
  selectedMapId = id;
  renderMapOptions();
  renderMapBuilderLibrary();
  els.mapBuilderStatus.textContent = `${map.name} saved and ready to play.`;
  els.mapBuilderSave.disabled = false;
}

function renderMapOptions() {
  els.mapCardGrid.innerHTML = allMapPresets().map((map, index) => {
    const selected = map.id === selectedMapId;
    const playable = isPlayableMap(map);
    const safeMapId = escapeHtml(map.id);
    const safePreview = /^[a-z0-9-]+$/i.test(map.preview)
      ? map.preview
      : "topology";
    const dimensions = map.tileCount
      ? `${map.tileCount} ${map.layout === "hex-odd-q" ? "HEX" : "MIXED"} TILES`
      : playable
        ? `${map.cols} × ${map.rows}`
        : "INVALID MAP";
    return `
      <button class="map-card${selected ? " selected" : ""}${playable ? "" : " topology-only"}" type="button" data-map-id="${safeMapId}" aria-pressed="${selected}">
        <span class="map-preview map-preview-${safePreview}" aria-hidden="true"><i></i><i></i><i></i></span>
        <span class="map-card-copy">
          <small>${String(index + 1).padStart(2, "0")} / ${escapeHtml(map.theater ?? "CUSTOM TOPOLOGY")}</small>
          <strong>${escapeHtml(map.name)}</strong>
          <span>${escapeHtml(map.description ?? "A mixed-polygon battlefield awaiting renderer support.")}</span>
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

function markStartupPerformance(name) {
  performance.mark(`rule-of-war:${name}`);
}

function measureStartupPerformance(name, start, end) {
  try {
    performance.measure(
      `rule-of-war:${name}`,
      `rule-of-war:${start}`,
      `rule-of-war:${end}`,
    );
  } catch {
    // A resumed or network-provided battlefield may not have local load marks.
  }
}

function showLoadingScreen(map) {
  els.loadingMapName.textContent = `${map.name} · ${map.cols} × ${map.rows} · ${(map.cols * map.rows).toLocaleString()} TILES`;
  updateLoadingProgress(0, "INITIALIZING COMMAND SYSTEMS");
  showGameMenu("loading");
}

function scheduleBattlefieldEnhancements(instance = battlefield3D) {
  if (!instance) return;
  const schedule =
    window.requestIdleCallback ??
    ((callback) =>
      window.setTimeout(
        () => callback({ didTimeout: false, timeRemaining: () => 8 }),
        120,
      ));
  schedule(() => {
    if (battlefield3D !== instance || instance.destroyed) return;
    markStartupPerformance("vegetation-start");
    instance.enableVegetation?.();
    markStartupPerformance("vegetation-ready");
    measureStartupPerformance(
      "vegetation-duration",
      "vegetation-start",
      "vegetation-ready",
    );
    schedule(() => {
      if (battlefield3D !== instance || instance.destroyed) return;
      markStartupPerformance("fog-clouds-start");
      instance.enableFogClouds?.();
      markStartupPerformance("fog-clouds-ready");
      measureStartupPerformance(
        "fog-clouds-duration",
        "fog-clouds-start",
        "fog-clouds-ready",
      );
      renderMap(battlefieldView, getUnit(state.selectedId), {
        dynamicOnly: true,
      });
    });
  });
}

function terrainCacheKey(map, seed, terrainConfig) {
  return `v3:${map.id}:${map.cols}x${map.rows}:${seed}:${JSON.stringify(terrainConfig ?? {})}`;
}

function generateTerrainInWorker(bounds, seed, config) {
  return new Promise((resolve, reject) => {
    const worker = new Worker(
      new URL("./terrain/terrain-worker.js", import.meta.url),
      { type: "module" },
    );
    worker.onmessage = ({ data }) => {
      worker.terminate();
      if (!data?.ok) {
        reject(new Error(data?.error ?? "Terrain worker failed."));
        return;
      }
      resolve({ terrain: data.terrain, raster: data.raster });
    };
    worker.onerror = (event) => {
      worker.terminate();
      reject(new Error(event.message || "Terrain worker failed."));
    };
    worker.postMessage({ bounds, seed, config, maxTextureSide: 512 });
  });
}

function rasterizeTerrainInWorker(serializedTerrain) {
  return new Promise((resolve, reject) => {
    const worker = new Worker(
      new URL("./terrain/terrain-worker.js", import.meta.url),
      { type: "module" },
    );
    worker.onmessage = ({ data }) => {
      worker.terminate();
      if (!data?.ok) {
        reject(new Error(data?.error ?? "Terrain raster worker failed."));
        return;
      }
      resolve(data.raster);
    };
    worker.onerror = (event) => {
      worker.terminate();
      reject(new Error(event.message || "Terrain raster worker failed."));
    };
    worker.postMessage({ serializedTerrain, maxTextureSide: 512 });
  });
}

async function prepareBattlefieldAssets(mapId) {
  markStartupPerformance("battlefield-load-start");
  const map = getMapPreset(mapId);
  showLoadingScreen(map);
  await yieldLoadingFrame();

  updateLoadingProgress(10, "TERRAIN · BUILDING COARSE PREVIEW");
  gameConfig = applyMapPreset(developerConfigStore.load(), mapId);
  activeTerrainSeed = null;
  const authoredTerrainData = map.authoredTerrain
    ? createAuthoredTerrainData(map)
    : null;
  applyRuntimeRules(
    authoredTerrainData
      ? { terrain: hydrateTerrain(authoredTerrainData) }
      : { resolutionScale: 0.08 },
  );
  battlefield3D?.destroy();
  battlefield3D = null;
  els.threeViewport.replaceChildren();
  document.body.classList.remove("three-enabled");
  battlefieldView = null;
  syncConfigDependentUI();
  await yieldLoadingFrame();

  const cacheKey = terrainCacheKey(map, activeTerrainSeed, map.terrainConfig);
  updateLoadingProgress(24, "TERRAIN · CHECKING SEED CACHE");
  let serializedTerrain = authoredTerrainData;
  let terrainRaster = null;
  if (!serializedTerrain)
    try {
      serializedTerrain = await getCachedTerrain(cacheKey);
    } catch (error) {
      console.warn("Terrain cache read skipped.", error);
    }
  if (!serializedTerrain && activeTerrainSeed === map.id) {
    updateLoadingProgress(30, "TERRAIN · STREAMING PREBUILT HEIGHT FIELD");
    try {
      const prebuilt = await loadPrebuiltTerrain(map.id);
      if (
        prebuilt?.width === runtimeMap.cols &&
        prebuilt?.height === runtimeMap.rows
      ) {
        serializedTerrain = prebuilt;
        cacheTerrain(cacheKey, serializedTerrain).catch((error) =>
          console.warn("Terrain cache write skipped.", error),
        );
      }
    } catch (error) {
      console.warn("Prebuilt terrain unavailable; using worker fallback.", error);
    }
  }
  if (!serializedTerrain) {
    updateLoadingProgress(32, "TERRAIN · GENERATING DETAILED HEIGHT FIELD");
    const generatedTerrain = await generateTerrainInWorker(
      runtimeMap.bounds,
      activeTerrainSeed,
      map.terrainConfig,
    );
    serializedTerrain = generatedTerrain.terrain;
    terrainRaster = generatedTerrain.raster;
    cacheTerrain(cacheKey, serializedTerrain).catch((error) =>
      console.warn("Terrain cache write skipped.", error),
    );
  } else updateLoadingProgress(58, "TERRAIN · RESTORING CACHED SEED");

  if (!terrainRaster)
    terrainRaster = await rasterizeTerrainInWorker(serializedTerrain);
  const detailedTerrain = hydrateTerrain(serializedTerrain);
  if (!detailedTerrain) throw new Error("Generated terrain could not be restored.");
  detailedTerrain.renderRaster = terrainRaster;
  applyRuntimeRules({ terrain: detailedTerrain });
  updateLoadingProgress(66, "WATER · LEVELING LAKES AND RIVERS");
  await yieldLoadingFrame();

  state = freshState();
  battlefieldView = null;
  updateLoadingProgress(76, "BATTLEFIELD · STREAMING TACTICAL WINDOW");
  const previewViewState = battlefield3D?.getViewState?.();
  initializeBattlefield3D({
    deferVegetation: true,
    deferFogClouds: true,
    viewState: previewViewState,
  });
  render();
  await yieldLoadingFrame();

  updateLoadingProgress(86, "VEGETATION · DEPLOYING LOW-POLY TREES");
  await yieldLoadingFrame();
  updateLoadingProgress(93, "FOG · FORMING CLOUD COVER");
  updateLoadingProgress(97, "BATTLEFIELD · DRAWING TACTICAL MAP");
  preparedMapId = map.id;
  markStartupPerformance("battlefield-core-ready");
  measureStartupPerformance(
    "battlefield-core-duration",
    "battlefield-load-start",
    "battlefield-core-ready",
  );
  await yieldLoadingFrame();
  updateLoadingProgress(100, "BATTLEFIELD · OPERATION READY");
  await yieldLoadingFrame();
  document.body.classList.remove("battlefield-loading-preview");
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
    document.body.classList.remove("battlefield-loading-preview");
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
  markStartupPerformance("battlefield-interactive");
  measureStartupPerformance(
    "time-to-interactive",
    "battlefield-load-start",
    "battlefield-interactive",
  );
  scheduleBattlefieldEnhancements();
}

async function startAiCampaign(mapId = selectedMapId) {
  closeMatchSocket(true);
  if (!activeWorldSectorId) activeTerrainModifications = [];
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

function sendRealtimeCommand(command) {
  if (!isHumanGame()) return false;
  const { type: commandType, ...payload } = command;
  return sendSocketMessage({
    type: "rts-command",
    sequence: ++multiplayer.sequence,
    commandType,
    ...payload,
  });
}

function applyRealtimeServerSnapshot(snapshot) {
  if (!isValidNetworkSnapshot(snapshot)) return false;
  const game = cloneJson(snapshot.game);
  const selectedIds = new Set(state.selectedIds ?? []);
  const selectedId = state.selectedId;
  Object.assign(state, game, {
    resolving: false,
    resolutionStep: null,
    selectedIds: game.units
      .filter((unit) => unit.owner === localOwner && selectedIds.has(unit.id))
      .map((unit) => unit.id),
    selectedId: game.units.some(
      (unit) => unit.id === selectedId && unit.owner === localOwner,
    )
      ? selectedId
      : null,
  });
  rtsRuntime = null;
  if (state.simulationTick - rtsLastRenderTick >= 4) {
    rtsLastRenderTick = state.simulationTick;
    if (
      !battlefieldView ||
      state.simulationTick - rtsLastFullRenderTick >= RTS_TICK_RATE
    ) {
      rtsLastFullRenderTick = state.simulationTick;
      render();
    } else {
      renderMap(battlefieldView, getUnit(state.selectedId), {
        dynamicOnly: true,
      });
    }
  }
  return true;
}

function handleMatchMessage(message) {
  switch (message.type) {
    case "session":
    case "sync":
      handleSessionMessage(message);
      break;
    case "state-update":
      if (message.source === "plan" && applyPlanDelta(message)) {
        saveMatchReference();
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
    case "simulation-snapshot":
      if (message.state) applyRealtimeServerSnapshot(message.state);
      break;
    case "rts-event":
      if (message.event) handleRtsEvent(message.event);
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

async function handleSessionMessage(message) {
  if (!isValidNetworkSnapshot(message.state)) {
    showConnectionError("The server returned an invalid campaign checkpoint.");
    return;
  }
  const resuming = multiplayer.pendingRequest?.type === "resume-match";
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
  if (resuming) {
    const resumeMap = findMapPresetByDimensions(
      message.state.config.cols,
      message.state.config.rows,
    );
    showLoadingScreen(resumeMap);
    updateLoadingProgress(18, "CHECKPOINT · DOWNLOADING SHARED OPERATION");
    await yieldLoadingFrame();
    updateLoadingProgress(42, "TERRAIN · RESTORING BATTLEFIELD SEED");
    await yieldLoadingFrame();
  }
  updateReadyState(message.ready, message.connected);
  updatePeerNames(message.players || []);
  applyNetworkSnapshot(message.state);
  if (resuming) {
    updateLoadingProgress(88, "BATTLEFIELD · SYNCHRONIZING COMMAND STATE");
    await yieldLoadingFrame();
  }
  saveMatchReference(message.state);
  updateLobbyStatus();

  if (message.pendingAdvance && !message.completed?.[localOwner]) {
    handlePhaseAdvance(message.pendingAdvance);
  } else if (message.pendingAdvance) {
    multiplayer.synchronizing = true;
  }
  render();
  if (resuming) {
    updateLoadingProgress(100, "CHECKPOINT · COMMAND LINK RESTORED");
    await yieldLoadingFrame();
  }
  maybeEnterHumanGame();
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

function applyPlanDelta(message) {
  const owner = Number(message.playerIndex);
  if (
    (owner !== 0 && owner !== 1) ||
    !Array.isArray(message.jobs) ||
    !Array.isArray(message.orders) ||
    !Number.isFinite(message.resources)
  )
    return false;
  state.jobs = [
    ...state.jobs.filter((item) => item.owner !== owner),
    ...cloneJson(message.jobs),
  ];
  state.orders = [
    ...state.orders.filter((item) => item.owner !== owner),
    ...cloneJson(message.orders),
  ];
  state.players[owner].resources = message.resources;
  return true;
}

function setMultiplayerReady() {
  if (isInteractionLocked()) return;
  sendPlanUpdate();
  closeStructurePopover();
  clearUnitSelection();
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
  if (message.rejected)
    log(
      "Phase results diverged. The server restored the pre-resolution checkpoint; submit the phase again.",
      true,
    );
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
  clearUnitSelection();
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
    return `Builds, launches, and recovers up to ${BUILDINGS.airfield.aircraftCapacity} bombers assigned to this airfield.`;
  if (structure.type === "mine")
    return `Feeds the supply network with ◆ ${gameConfig.mineIncome} at the end of every round.`;
  if (structure.type === "fieldHospital")
    return "Automatically restores one nearby friendly formation after each round.";
  if (structure.type === "bridge")
    return "Creates an operational crossing that allows ground units to traverse water cells.";
  if (structure.type === "watchtower")
    return "A passive observation post that spots targets for long-range fire support.";
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
    friendly &&
    ["deployment", "realtime"].includes(state.phase) &&
    !isInteractionLocked();
  const buttons = recruitableTypes
    .map(([type, spec]) => {
      const producerReady = producerCanRecruit(structure, type, localOwner);
      const affordable = state.players[localOwner].resources >= spec.price;
      const disabled = !controlsAvailable || !producerReady || !affordable;
      const reason = !friendly
        ? "Enemy production is unavailable."
        : !["deployment", "realtime"].includes(state.phase)
          ? "Recruitment is unavailable."
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
    : !["deployment", "realtime"].includes(state.phase)
      ? "Recruitment controls are unavailable."
      : structure.type === "airfield" &&
          !producerCanRecruit(structure, "plane", localOwner)
        ? `This airfield's ${BUILDINGS.airfield.aircraftCapacity} aircraft bays are occupied or reserved.`
        : "Recruits enter the manifest now and deploy after round resolution.";
  return `<div class="structure-production"><div class="structure-production-heading"><strong>PRODUCTION</strong><span>${queuedCount ? `${queuedCount} QUEUED` : "READY NEXT ROUND"}</span></div><div class="structure-recruit-grid">${buttons}</div><p class="structure-production-note">${note}</p></div>`;
}

function positionBattlefieldPopover(element, anchor = null) {
  const point = anchor ?? {
    x: els.battlefield.clientWidth / 2,
    y: els.battlefield.clientHeight / 2,
  };
  const margin = 10;
  const width = element.offsetWidth;
  const height = element.offsetHeight;
  let left = point.x + 14;
  if (left + width + margin > els.battlefield.clientWidth)
    left = point.x - width - 14;
  left = Math.max(
    margin,
    Math.min(left, els.battlefield.clientWidth - width - margin),
  );
  const top = Math.max(
    margin,
    Math.min(
      point.y - height / 2,
      els.battlefield.clientHeight - height - margin,
    ),
  );
  element.style.left = `${left}px`;
  element.style.top = `${top}px`;
}

function renderStructurePopover() {
  const structure = getStructure(selectedStructureId);
  const commander = getUnit(state.selectedId);
  if (
    !structure &&
    commander?.owner === localOwner &&
    commander.type === "commander" &&
    commander.hp > 0 &&
    state.phase === "deployment"
  ) {
    const watchtower = BUILDINGS.watchtower;
    const bridge = BUILDINGS.bridge;
    const locked = isInteractionLocked();
    els.structurePopover.className = "structure-popover commander-popover";
    els.structurePopover.innerHTML = `<header class="structure-popover-header"><span class="structure-popover-mark" aria-hidden="true">${UNIT_TYPES.commander.mark}</span><span class="structure-popover-heading"><small>FIELD COMMANDER</small><strong id="structure-popover-title">COMMAND POST</strong></span><button class="structure-popover-close" type="button" data-close-commander aria-label="Close commander controls">×</button></header><div class="structure-popover-body"><p class="structure-popover-description">Commission field structures within ${movementRangeFor(commander)} cells of this commander.</p><div class="structure-stat-grid"><span class="structure-stat"><small>POSITION</small><strong>${coord(commander.x, commander.y)}</strong></span><span class="structure-stat"><small>INTEGRITY</small><strong>${commander.hp} / ${UNIT_TYPES.commander.lives} HP</strong></span></div><div class="structure-production"><div class="structure-production-heading"><strong>FIELD CONSTRUCTION</strong><span>COMMANDER ONLY</span></div><div class="structure-recruit-grid"><button class="structure-recruit${state.buildMode === "watchtower" ? " active" : ""}" type="button" data-commander-build="watchtower"${locked || state.players[localOwner].resources < watchtower.price ? " disabled" : ""}><i>${watchtower.mark}</i><strong>${watchtower.label}</strong><span>♦ ${watchtower.price}</span></button><button class="structure-recruit${state.buildMode === "bridge" ? " active" : ""}" type="button" data-commander-build="bridge"${locked || state.players[localOwner].resources < bridge.pricePerMeter ? " disabled" : ""}><i>${bridge.mark}</i><strong>${bridge.label}</strong><span>♦ ${bridge.pricePerMeter}/M</span></button></div><p class="structure-production-note">Watchtowers occupy one cell. Bridges are priced by their final span.</p></div></div>`;
    els.structurePopover.hidden = false;
    positionBattlefieldPopover(els.structurePopover, structurePopoverAnchor);
    return;
  }
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

  positionBattlefieldPopover(els.structurePopover, structurePopoverAnchor);
}

function render() {
  const selected = getUnit(state.selectedId);
  const viewUiSignature = `${state.selectedId}:${(state.selectedIds ?? []).join(",")}:${state.intent}:${state.buildMode}:${state.bridgeStart?.x ?? ""}:${state.bridgeStart?.y ?? ""}`;
  const refreshView =
    !battlefieldView ||
    !isRealtimeGame() ||
    state.simulationTick - battlefieldViewTick >= RTS_TICK_RATE / 2 ||
    viewUiSignature !== battlefieldViewUiSignature;
  if (refreshView) {
    battlefieldView = createBattlefieldViewIndex({
      state,
      runtimeMap,
      localOwner,
      fogEnabled: gameConfig.fogEnabled,
      unitTypes: UNIT_TYPES,
      buildings: BUILDINGS,
      selected,
      movementRangeFor,
      getWorldPoint: mapPositionFor,
      canUnitAttack,
      isValidBuildTile,
      isTerrainPassable: (x, y) =>
        Number.isFinite(groundMovementCostAt(x, y)),
    });
    battlefieldViewTick = state.simulationTick;
    battlefieldViewUiSignature = viewUiSignature;
  }
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
  const selectedIds = selectedUnitIdSet();
  const nextUiSignature = `${[...selectedIds].join(",")}:${els.navigationHud?.classList.contains("collapsed")}`;
  const nextRenderSignature = [
    nextUiSignature,
    ...state.units.map(
      (unit) =>
        `${unit.id}:${Number(unit.worldX ?? unit.x).toFixed(1)}:${Number(unit.worldY ?? unit.y).toFixed(1)}:${unit.hp}`,
    ),
    ...state.structures.map(
      (structure) =>
        `${structure.id}:${structure.owner}:${structure.hp}`,
    ),
    ...view.visibilitySources.map(
      (source) =>
        `${source.id}:${source.x.toFixed(1)}:${source.y.toFixed(1)}:${source.radius}`,
    ),
  ].join("|");
  if (nextRenderSignature === navigationRenderSignature) return;
  const now = performance.now();
  if (
    nextUiSignature === navigationUiSignature &&
    now - navigationLastRenderTime < 250
  )
    return;
  navigationRenderSignature = nextRenderSignature;
  navigationUiSignature = nextUiSignature;
  navigationLastRenderTime = now;
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
          return `<button class="unit-nav-row${selectedIds.has(unit.id) ? " active" : ""}" type="button" data-unit-nav="${unit.id}" aria-label="Focus ${spec.label} at ${coord(unit.x, unit.y)}"><span class="unit-nav-mark">${spec.mark}</span><span class="unit-nav-copy"><strong>${spec.label}</strong><small>${coord(unit.x, unit.y)} / ${status}</small></span><b class="unit-nav-health">${unit.hp}/${spec.lives}</b></button>`;
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
    const center = mapPositionFor(piece);
    let point = project(center);
    const friendly = piece.owner === localOwner;
    const isUnit = piece.type in UNIT_TYPES;
    const selected =
      selectedIds.has(piece.id) || piece.id === selectedStructureId;
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
  if (!isInteractionLocked()) {
    selectUnit(unit);
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
  const elapsed = Math.floor(Number(state.elapsedSeconds ?? 0));
  els.round.textContent = `${String(Math.floor(elapsed / 60)).padStart(2, "0")}:${String(elapsed % 60).padStart(2, "0")}`;
  els.resources.textContent = state.players[localOwner].resources;
  els.income.textContent = `+${Math.round(incomeFor(localOwner) * 6)} / min`;
  els.enemyForce.textContent = visibleEnemies.length
    ? `${visibleUnits} UNIT${visibleUnits === 1 ? "" : "S"} · ${visibleBarracks} BARRACKS SIGHTED`
    : "NO VISIBLE CONTACTS";
}

function renderPhase() {
  document.body.classList.toggle("rts-mode", isRealtimeGame());
  document.body.classList.toggle("rts-paused", isRealtimeGame() && rtsPaused);
  if (isRealtimeGame()) {
    els.deploymentControls.hidden = false;
    els.orderControls.hidden = false;
    els.phaseIndex.textContent = "RT";
    els.phaseName.textContent = state.winner ? "COMPLETE" : "LIVE COMMAND";
    els.phaseRing.textContent = state.winner ? "■" : "●";
    els.phaseButton.hidden = true;
    els.trackDeployment.hidden = true;
    els.trackOrders.hidden = true;
    els.trackAttack.hidden = true;
    els.trackResolve.hidden = true;
    els.orderSectionTitle.textContent = "REAL-TIME ORDERS";
    els.orderSectionLimit.textContent = "RIGHT-CLICK TO COMMAND";
    els.orderSectionHint.textContent =
      "Units move continuously and automatically engage visible hostiles. Use attack-move to sweep contested ground.";
    els.moveButton.hidden = false;
    els.attackButton.hidden = false;
    els.turnSkipControl.hidden = true;
    document.body.classList.remove("resolving");
    return;
  }
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
  if (els.turnSkipControl)
    els.turnSkipControl.hidden = isHumanGame();
  if (els.turnSkipButton) {
    els.turnSkipButton.disabled =
      isHumanGame() || isInteractionLocked() || Boolean(state.winner);
    els.turnSkipButton.textContent = fastForwarding || state.resolving
      ? "ADVANCING..."
      : "FAST FORWARD";
  }
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
  const centers = friendlyPieces.map(mapPositionFor);
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
  const sourceFor = (piece, radius) => ({
    ...mapPositionFor(piece),
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
  const group = selectedUnits();
  if (isRealtimeGame() && group.length) {
    return {
      kind: "targetable",
      sources: group
        .filter((unit) => Number(UNIT_TYPES[unit.type]?.range) > 0)
        .map((unit) => sourceFor(unit, UNIT_TYPES[unit.type].range)),
      secondary: {
        kind: "sight",
        sources: group.map((unit) => {
          const spec = UNIT_TYPES[unit.type];
          return sourceFor(
            unit,
            spec.visionRange ?? spec.acquisitionRange ?? spec.range ?? 0,
          );
        }),
      },
    };
  }
  if (group.length > 1 && state.intent === "move")
    return {
      kind: "reachable",
      sources: group.map((unit) => sourceFor(unit, movementRangeFor(unit))),
    };
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

function renderMap(
  view,
  selected = getUnit(state.selectedId),
  { dynamicOnly = false } = {},
) {
  const selectedStructure = getStructure(selectedStructureId);
  const selectedIds = selectedUnitIdSet();
  const renderFallbackGrid = !battlefield3D;
  const tiles = renderFallbackGrid ? [] : null;
  const fallbackBudget = isVirtualizedBattlefield()
    ? MAX_FALLBACK_TILES
    : runtimeMap.tileCount;
  const mapTiles = dynamicOnly
    ? []
    : renderTileDescriptors(
        view,
        renderFallbackGrid ? fallbackBudget : MAX_DETAILED_TILES,
      );
  const tileModels = [];
  for (const mapTile of mapTiles) {
    const { x, y } = mapTile;
    const key = `${x},${y}`;
    const visible = view.isVisible(x, y);
    const isSelected = Boolean(
      (!isRealtimeGame() &&
        state.units.some(
        (unit) =>
          selectedIds.has(unit.id) && unit.x === x && unit.y === y,
        )) ||
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
    .filter(
      (piece) => piece.type === "objective" || isPieceVisible(piece, view),
    )
    .map((piece) => {
      const isUnit = piece.type in UNIT_TYPES;
      const spec = isUnit ? UNIT_TYPES[piece.type] : BUILDINGS[piece.type];
      const progressFor = (job) =>
        Math.max(
          0,
          Math.min(
            1,
            Number(job?.progressTicks ?? 0) /
              Math.max(
                1,
                Number(
                  job?.durationTicks ??
                    Number(job?.durationSeconds ?? 8) * RTS_TICK_RATE,
                ),
              ),
          ),
        );
      const producerJobs =
        !isUnit && piece.kind !== "building" && piece.type === "barracks"
          ? state.jobs.filter(
              (job) =>
                job.kind === "recruit" &&
                (job.producerId ?? job.barracksId) === piece.id,
            )
          : [];
      const productionProgress = producerJobs.length
        ? Math.max(...producerJobs.map(progressFor))
        : piece.kind === "building"
          ? progressFor(piece)
          : null;
      const order =
        isUnit && piece.owner === localOwner && view.orderByUnit.get(piece.id);
      return {
        id: piece.id,
        kind: isUnit ? "unit" : "structure",
        type: piece.type,
        owner: piece.owner === localOwner ? 0 : 1,
        objectiveControl:
          piece.type !== "objective"
            ? null
            : piece.owner < 0
              ? "neutral"
              : piece.owner === localOwner
                ? "friendly"
                : "enemy",
        x: piece.x,
        y: piece.y,
        hp: piece.hp || 1,
        maxHp: spec.lives,
        pending: piece.kind === "building",
        productionProgress,
        productionCount: producerJobs.length,
        guide: piece.guide === true,
        bridgeAnchorOnly: piece.bridgeAnchorOnly === true,
        selected:
          selectedIds.has(piece.id) || piece.id === selectedStructureId,
        orderKind: order?.commandType ?? order?.kind ?? null,
        heading: Number.isFinite(piece.heading)
          ? piece.heading
          : piece.owner === 0
            ? 0
            : Math.PI,
        airborne: piece.airborne === true,
        airState: piece.airState ?? null,
        fuel: Number.isFinite(piece.fuel) ? piece.fuel : null,
        maxFuel: Number.isFinite(piece.maxFuel) ? piece.maxFuel : null,
        ...(isUnit &&
        Number.isFinite(piece.worldX) &&
        Number.isFinite(piece.worldY)
          ? {
              worldX: piece.worldX,
              worldY: piece.worldY,
              velocityX: Number(piece.velocityX) || 0,
              velocityY: Number(piece.velocityY) || 0,
            }
          : {}),
        footprintSize: isUnit ? 1 : (spec.footprintSize ?? 1),
        ...(!isUnit && Number.isFinite(spec.modelWidth)
          ? { modelWidth: spec.modelWidth }
          : {}),
        ...(!isUnit && Number.isFinite(spec.modelDepth)
          ? { modelDepth: spec.modelDepth }
          : {}),
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
    tiles: dynamicOnly ? null : tileModels,
    pieces,
    orderTargets: state.orders
      .filter(
        (order) =>
          order.owner === localOwner &&
          (order.kind === "attack" ||
            order.kind === "move" ||
            order.kind === "command"),
      )
      .filter((order) => {
        if (order.kind === "attack" || order.commandType === "attack")
          return true;
        const actor = getUnit(order.unitId);
        return UNIT_TYPES[actor?.type]?.airUnit === true;
      })
      .map((order) => {
        const actor = getUnit(order.unitId);
        return {
          id: order.id,
          kind:
            order.kind === "attack" || order.commandType === "attack"
              ? "attack"
              : UNIT_TYPES[actor?.type]?.airUnit === true
                ? "bomb"
                : "move",
          x: order.x,
          y: order.y,
          worldX: order.worldX,
          worldY: order.worldY,
        };
      }),
    movementRoutes: state.orders
      .filter(
        (order) =>
          order.owner === localOwner &&
          (order.kind === "move" ||
            (order.kind === "command" &&
              ["move", "attack-move", "patrol"].includes(order.commandType))),
      )
      .map((order) => {
        const actor = getUnit(order.unitId);
        if (!actor || UNIT_TYPES[actor.type]?.airUnit === true) return null;
        if (order.kind === "command") {
          const commands = state.orders.filter(
            (candidate) =>
              candidate.kind === "command" &&
              candidate.unitId === actor.id &&
              ["move", "attack-move", "patrol"].includes(
                candidate.commandType,
              ),
          );
          const commandIndex = commands.findIndex(
            (candidate) => candidate.id === order.id,
          );
          const previousCommand =
            commandIndex > 0 ? commands[commandIndex - 1] : null;
          const points =
            commandIndex <= 0
              ? [
                  mapPositionFor(actor),
                  ...actor.path.slice(actor.pathIndex ?? 0),
                ]
              : [
                  { x: previousCommand.x, y: previousCommand.y },
                  { x: order.x, y: order.y },
                ];
          if (!points.length && Number.isFinite(order.x))
            points.push({ x: order.x, y: order.y });
          return {
            id: order.id,
            points,
            flagPoints: [{ x: order.x, y: order.y }],
          };
        }
        const waypointIndex = Number.isInteger(order.waypointIndex)
          ? order.waypointIndex
          : 0;
        const remainingWaypoints = Array.isArray(order.waypoints)
          ? order.waypoints.slice(waypointIndex)
          : [{ x: order.x, y: order.y }];
        const actorPathIndex = Array.isArray(order.path)
          ? order.path.findIndex(
              (point) => {
                if (point.x !== actor.x || point.y !== actor.y) return false;
                if (
                  Number.isFinite(point.worldX) &&
                  Number.isFinite(point.worldY)
                )
                  return (
                    euclideanDistance(mapPositionFor(actor), {
                      x: point.worldX,
                      y: point.worldY,
                    }) < 0.05
                  );
                return true;
              },
            )
          : -1;
        const remainingPath = Array.isArray(order.path)
          ? order.path.slice(actorPathIndex >= 0 ? actorPathIndex + 1 : 0)
          : remainingWaypoints;
        return {
          id: order.id,
          points: [
            {
              x: actor.x,
              y: actor.y,
              worldX: actor.worldX,
              worldY: actor.worldY,
            },
            ...remainingPath.map((point) => ({
              x: point.x,
              y: point.y,
              worldX: point.worldX,
              worldY: point.worldY,
            })),
          ],
          flagPoints: remainingWaypoints.map((waypoint) => ({
            x: waypoint.x,
            y: waypoint.y,
            worldX: waypoint.worldX,
            worldY: waypoint.worldY,
          })),
        };
      })
      .filter(Boolean),
    routePlanning:
      Boolean(selected) &&
      state.intent === "move" &&
      UNIT_TYPES[selected.type]?.airUnit !== true,
    fog: {
      enabled: gameConfig.fogEnabled,
      sources: view.visibilitySources,
    },
    rangeOverlay: virtualRangeOverlay(selected),
    terrainFlatteningAreas: terrainFlatteningAreas(),
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

function terrainFlatteningAreas() {
  const buildings = [
    ...state.structures,
    ...state.jobs.filter((job) => job.kind === "building"),
  ].filter(
    (piece) =>
      piece.type !== "bridge" &&
      Array.isArray(piece.footprint) &&
      piece.footprint.length,
  );
  const areas = buildings
    .map((piece) => {
      const points = [];
      const elevations = [];
      for (const tile of piece.footprint) {
        const descriptor =
          runtimeMap.tileByCoordinate.get(`${tile.x},${tile.y}`) ?? null;
        if (descriptor?.vertices?.length) points.push(...descriptor.vertices);
        else {
          const center = tileCenter(tile.x, tile.y);
          points.push(
            { x: center.x - 0.5, y: center.y - 0.5 },
            { x: center.x + 0.5, y: center.y + 0.5 },
          );
        }
        elevations.push(terrainProperties(tile.x, tile.y).surfaceElevation);
      }
      if (!points.length || !elevations.length) return null;
      elevations.sort((first, second) => first - second);
      const targetElevation = elevations[Math.floor(elevations.length / 2)];
      // Keep one complete cell of level ground around every foundation. This
      // prevents the smoothing falloff from cutting into the building pad.
      const foundationMargin = 1;
      return {
        id: piece.id,
        minX: Math.min(...points.map((point) => point.x)) - foundationMargin,
        maxX: Math.max(...points.map((point) => point.x)) + foundationMargin,
        minY: Math.min(...points.map((point) => point.y)) - foundationMargin,
        maxY: Math.max(...points.map((point) => point.y)) + foundationMargin,
        targetElevation,
        edge: 1.25,
      };
    })
    .filter(Boolean);
  // Adjacent foundation buffers must share one plateau. Leaving overlapping
  // areas at independent levels causes the later area's elevation to win and
  // creates a ridge between otherwise level structures.
  const merged = [];
  for (const area of areas) {
    const matches = merged.filter(
      (other) =>
        area.minX <= other.maxX &&
        area.maxX >= other.minX &&
        area.minY <= other.maxY &&
        area.maxY >= other.minY,
    );
    if (!matches.length) {
      merged.push({ ...area, levels: [area.targetElevation] });
      continue;
    }
    const target = matches[0];
    target.minX = Math.min(target.minX, area.minX);
    target.maxX = Math.max(target.maxX, area.maxX);
    target.minY = Math.min(target.minY, area.minY);
    target.maxY = Math.max(target.maxY, area.maxY);
    target.levels.push(area.targetElevation);
    for (const match of matches.slice(1)) {
      target.minX = Math.min(target.minX, match.minX);
      target.maxX = Math.max(target.maxX, match.maxX);
      target.minY = Math.min(target.minY, match.minY);
      target.maxY = Math.max(target.maxY, match.maxY);
      target.levels.push(...match.levels);
      merged.splice(merged.indexOf(match), 1);
    }
  }
  return merged.map((area, index) => {
    const levels = area.levels.sort((first, second) => first - second);
    return {
      ...area,
      id: `foundation-${index}`,
      targetElevation: levels[Math.floor(levels.length / 2)],
      levels: undefined,
    };
  });
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
  const group = selectedUnits();
  if (isRealtimeGame()) {
    renderRealtimeSelection(selected, group);
    return;
  }
  const groupSelected = group.length > 1;
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
  } else if (groupSelected) {
    const groundGroup = group.every(
      (unit) => UNIT_TYPES[unit.type]?.airUnit !== true,
    );
    const totalMoveCost = group.reduce(
      (sum, unit) => sum + (UNIT_TYPES[unit.type]?.actionCost ?? 0),
      0,
    );
    const blockedCount = group.filter(
      (unit) => !canQueueUnitOrder(unit, "move"),
    ).length;
    els.selectedCard.className = "selected-card";
    els.selectedCard.innerHTML = `<span class="selected-unit-icon">${group.length}</span><span><strong>PLATOON SELECTED</strong><small>${group.length} unit${group.length === 1 ? "" : "s"} · ◆ ${totalMoveCost} to move all · Shift-click to add/remove</small></span>`;
    els.moveButton.disabled =
      state.phase !== "orders" ||
      !groundGroup ||
      blockedCount > 0 ||
      state.players[localOwner].resources < totalMoveCost ||
      isInteractionLocked();
    els.attackButton.disabled = true;
    els.attackButton.title = "Group orders currently support movement only.";
    els.attackRange.textContent = "Group movement";
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
    const formationStatus =
      Number.isInteger(selected.strength) && selected.maxStrength > 1
        ? ` · ${selected.strength}/${selected.maxStrength} STRENGTH`
        : "";
    els.selectedCard.innerHTML = `<span class="selected-unit-icon">${spec.mark}</span><span><strong>${spec.label} UNIT · ${coord(selected.x, selected.y)}</strong><small>${selected.hp}/${spec.lives} HP${formationStatus} · ${damageFor(selected)} DMG · ◆ ${spec.actionCost} per ${selectedIsAirUnit ? "run" : "action"}${ammunitionStatus}${queuedStatus}${movementStatus}</small></span>`;
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
  if (moveLabel)
    moveLabel.textContent = groupSelected
      ? "PLATOON MOVE"
      : selectedIsAirUnit
        ? "BOMB RUN"
        : "MOVE";
  if (moveCopy)
    moveCopy.textContent = groupSelected
      ? `${group.length} units selected`
      : selected
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
    message = `Place ${BUILDINGS[state.buildMode].label}'s ${BUILDINGS[state.buildMode].footprintSize} × ${BUILDINGS[state.buildMode].footprintSize} footprint on a highlighted square.`;
  else if (selectedStructureId) {
    const selectedStructure = getStructure(selectedStructureId);
    if (selectedStructure)
      message = `${BUILDINGS[selectedStructure.type].label} selected at ${coord(selectedStructure.x, selectedStructure.y)}.`;
  }
  else if (groupSelected && state.intent === "move")
    message = `Choose a platoon destination. Units will spread into formation and plot individual routes.`;
  else if (groupSelected)
    message = `${group.length} units selected. Shift-click units to add or remove them.`;
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

function renderRealtimeSelection(selected, group) {
  const groupSelected = group.length > 1;
  if (!selected) {
    els.selectedCard.className = "selected-card empty";
    els.selectedCard.innerHTML =
      '<span class="selected-unit-icon">—</span><span><strong>NO UNIT SELECTED</strong><small>Click or drag-select friendly units.</small></span>';
  } else if (groupSelected) {
    els.selectedCard.className = "selected-card";
    els.selectedCard.innerHTML = `<span class="selected-unit-icon">${group.length}</span><span><strong>COMBAT GROUP</strong><small>${group.length} units · Shift queues waypoints · E attack · F flee · H hold fire</small></span>`;
  } else {
    const spec = UNIT_TYPES[selected.type];
    const stats = rtsStatsFor(spec, RTS_TICK_RATE);
    const cooldown = Math.max(
      0,
      Number(selected.weaponCooldownTicks ?? 0) / RTS_TICK_RATE,
    );
    const command = ensureRtsRuntime().commandFor(selected.id);
    els.selectedCard.className = "selected-card";
    const fuelStatus = spec.airUnit
      ? ` · ${Math.round(
          (Number(selected.fuel ?? spec.fuelCapacity) /
            Math.max(1, Number(selected.maxFuel ?? spec.fuelCapacity))) *
            100,
        )}% FUEL · ${String(selected.airState ?? "landed").toUpperCase()}`
      : "";
    els.selectedCard.innerHTML = `<span class="selected-unit-icon">${spec.mark}</span><span><strong>${spec.label} · ${coord(selected.x, selected.y)}</strong><small>${selected.hp}/${spec.lives} HP · ${stats.speed.toFixed(1)} SPEED · ${spec.range} RANGE · ${cooldown.toFixed(1)}s RELOAD${fuelStatus}${command ? ` · ${command.commandType.toUpperCase()}` : ""}</small></span>`;
  }
  const available = Boolean(selected) && !isInteractionLocked();
  els.moveButton.disabled = !available;
  els.attackButton.disabled = !available;
  if (els.stopButton) els.stopButton.disabled = !available;
  if (els.engageButton) {
    els.engageButton.disabled = !available;
    els.engageButton.classList.toggle(
      "active",
      group.some((unit) => unit.stance === "attack"),
    );
  }
  if (els.fleeButton) {
    els.fleeButton.disabled = !available;
    els.fleeButton.classList.toggle(
      "active",
      group.some((unit) => unit.stance === "flee"),
    );
  }
  if (els.holdButton) els.holdButton.disabled = !available;
  els.holdButton?.classList.toggle(
    "active",
    group.some((unit) => unit.stance === "hold-fire"),
  );
  els.moveButton.hidden = false;
  els.attackButton.hidden = false;
  els.moveButton.querySelector("strong").textContent = "MOVE";
  els.moveButton.querySelector("small").textContent = "M · right-click";
  els.attackButton.querySelector("strong").textContent = "ATTACK-MOVE";
  els.attackRange.textContent = "A · auto engage";
  els.moveButton.classList.toggle("active", state.intent === "move");
  els.attackButton.classList.toggle("active", state.intent === "attack-move");
  let message = "Select units, then right-click terrain to move or enemies to focus fire.";
  if (state.buildMode)
    message = `Place ${BUILDINGS[state.buildMode].label} on valid terrain. Construction completes in real time.`;
  else if (state.intent === "move")
    message = "Choose a destination. Shift preserves the current command queue.";
  else if (state.intent === "attack-move")
    message = "Choose ground to sweep. Units will stop and engage contacts en route.";
  else if (groupSelected)
    message = `${group.length} units ready. E attack · F flee · H hold fire · Shift queues waypoints.`;
  else if (selected)
    message = `${UNIT_TYPES[selected.type].label} ready · stance ${String(selected.stance).replace("-", " ")}.`;
  els.selectionReadout.innerHTML = `<span class="selection-icon">⌖</span><span><small>LIVE COMMAND</small><strong>${message}</strong></span>`;
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
        const progress = Math.min(
          100,
          Math.round(
            (Number(item.progressTicks ?? 0) /
              Math.max(1, Number(item.durationTicks ?? RTS_TICK_RATE))) *
              100,
          ),
        );
        return queueMarkup(
          item.id,
          UNIT_TYPES[item.type].mark,
          `${UNIT_TYPES[item.type].label} RECRUIT`,
          `Training ${progress}%`,
          item.cost,
          "build",
        );
      }
      if (item.kind === "command") {
        const actor = getUnit(item.unitId);
        const unitName = actor ? UNIT_TYPES[actor.type].label : "UNIT";
        const destination = Number.isFinite(item.x)
          ? coord(Math.round(item.x), Math.round(item.y))
          : "designated target";
        return queueMarkup(
          item.id,
          item.commandType === "attack" ? "⌖" : "↗",
          `${unitName} · ${item.commandType.toUpperCase()}`,
          item.commandType === "attack"
            ? "Focus hostile target"
            : `Destination ${destination}`,
          null,
          "action",
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
  const costLabel = cost == null
    ? "LIVE"
    : recurring
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

function isFieldBuilder(unit, owner = localOwner) {
  return Boolean(
    unit &&
      unit.owner === owner &&
      unit.hp > 0 &&
      (unit.type === "commander" || UNIT_TYPES[unit.type]?.engineer === true),
  );
}

function renderPurchases() {
  const deployment = state.phase === "deployment" || isRealtimeGame();
  const builderSelected = isFieldBuilder(getUnit(state.selectedId));
  if (els.commanderConstruction)
    els.commanderConstruction.hidden = !builderSelected;
  const hasBuilder = state.units.some((item) => isFieldBuilder(item));
  document.querySelectorAll("[data-build]").forEach((button) => {
    const type = button.dataset.build;
    const spec = BUILDINGS[type];
    const minimumPrice = Number.isFinite(spec.price)
      ? spec.price
      : Math.max(1, Math.round(spec.pricePerMeter));
    button.disabled =
      !deployment ||
      isInteractionLocked() ||
      (spec.commanderOnly && (!hasBuilder || !builderSelected)) ||
      state.players[localOwner].resources < minimumPrice;
    button.classList.toggle("active", state.buildMode === type);
    button.title =
      spec.commanderOnly && !builderSelected
        ? "Select a surviving commander or engineer to construct this."
        : "";
  });
}

function chooseBuild(type) {
  const spec = BUILDINGS[type];
  const minimumPrice = Number.isFinite(spec?.price)
    ? spec.price
    : Math.max(1, Math.round(spec?.pricePerMeter ?? 0));
  const hasBuilder = state.units.some((item) => isFieldBuilder(item));
  if (
    !spec ||
    !["deployment", "realtime"].includes(state.phase) ||
    isInteractionLocked() ||
    (spec.commanderOnly &&
      (!hasBuilder || !isFieldBuilder(getUnit(state.selectedId)))) ||
    state.players[localOwner].resources < minimumPrice
  )
    return;
  state.buildMode = state.buildMode === type ? null : type;
  state.bridgeStart = null;
  if (!spec.commanderOnly) {
    clearUnitSelection();
    closeStructurePopover();
  } else selectedStructureId = null;
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
      .filter((item) => isFieldBuilder(item, owner))
      .filter(
        (item) =>
          owner !== localOwner ||
          !isFieldBuilder(getUnit(state.selectedId), owner) ||
          item.id === state.selectedId,
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
      isFieldBuilder(item, owner),
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
  const startCenter = tileCenter(start.x, start.y);
  const endCenter = tileCenter(x, y);
  const direction = {
    x: (endCenter.x - startCenter.x) / lengthMeters,
    y: (endCenter.y - startCenter.y) / lengthMeters,
  };
  const interior = footprint.filter((tile) => {
    const center = tileCenter(tile.x, tile.y);
    const projection =
      (center.x - startCenter.x) * direction.x +
      (center.y - startCenter.y) * direction.y;
    return projection > 0.55 && projection < lengthMeters - 0.55;
  });
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
  const spec = BUILDINGS[type];
  const footprint = buildingFootprintCoordinates(x, y, type);
  if (
    !isFootprintClear(footprint, {
      expectedSize: (spec?.footprintSize ?? 3) ** 2,
    }) ||
    !footprint.every((tile) => {
      const terrain = terrainProperties(tile.x, tile.y);
      return terrain.passable && !terrain.water;
    })
  )
    return false;
  if (spec?.commanderOnly)
    return state.units.some(
      (commander) =>
        commander.id === state.selectedId &&
        isFieldBuilder(commander, owner) &&
        navigationGraph.isConnected(commander, { x, y }) &&
        distance(commander, { x, y }) <= movementRangeFor(commander),
    );
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
    .filter((item) => isFieldBuilder(item, owner))
    .some(
      (commander) =>
        navigationGraph.isConnected(commander, { x, y }) &&
        distance(commander, { x, y }) <= movementRangeFor(commander),
    );
}

function queueBuilding(type, x, y, owner = localOwner) {
  const spec = BUILDINGS[type];
  if (!spec || spec.linear || !Number.isFinite(spec.price)) return false;
  const footprint = buildingFootprintCoordinates(x, y, type);
  if (
    (owner === localOwner &&
      (!["deployment", "realtime"].includes(state.phase) || isInteractionLocked())) ||
    !isValidBuildTile(x, y, owner, type) ||
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
    durationSeconds: type === "watchtower" ? 5 : 10,
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
      (!["deployment", "realtime"].includes(state.phase) || isInteractionLocked())) ||
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
    durationSeconds: Math.max(8, metrics.lengthMeters * 0.5),
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
      (!["deployment", "realtime"].includes(state.phase) || isInteractionLocked())) ||
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
    durationSeconds: spec.airUnit ? 18 : 8 + spec.price * 0.2,
  });
  if (owner === localOwner) {
    log(`${spec.label} unit added to the reinforcement manifest.`);
    schedulePlanSync();
  }
  render();
  return true;
}

function nearestFriendlyUnitAt(x, y, pointer = null) {
  const target = targetPointForTile(x, y, pointer);
  return (
    state.units
      .filter((unit) => {
        if (unit.owner !== localOwner || unit.hp <= 0) return false;
        const separation = euclideanDistance(mapPositionFor(unit), target);
        const selectionRadius = UNIT_TYPES[unit.type]?.airUnit ? 0.72 : 0.48;
        return separation <= selectionRadius || (unit.x === x && unit.y === y);
      })
      .sort((first, second) => {
        const distanceDifference =
          euclideanDistance(mapPositionFor(first), target) -
          euclideanDistance(mapPositionFor(second), target);
        if (Math.abs(distanceDifference) > 0.01) return distanceDifference;
        return (
          Number(UNIT_TYPES[second.type]?.airUnit === true) -
          Number(UNIT_TYPES[first.type]?.airUnit === true)
        );
      })[0] ?? null
  );
}

function formationOffsets(count) {
  if (count <= 1) return [{ x: 0, y: 0 }];
  const offsets = [{ x: 0, y: 0 }];
  for (let ring = 1; offsets.length < count; ring += 1) {
    const slots = ring === 1 ? 6 : ring * 8;
    for (let index = 0; index < slots && offsets.length < count; index += 1) {
      const angle = (index / slots) * Math.PI * 2;
      offsets.push({
        x: Math.cos(angle) * FORMATION_SPACING * ring,
        y: Math.sin(angle) * FORMATION_SPACING * ring,
      });
    }
  }
  return offsets;
}

function isReservedFormationPoint(point, reserved) {
  return reserved.some(
    (other) =>
      euclideanDistance(point, other) < UNIT_COLLISION_RADIUS * 2.2,
  );
}

function findFormationDestination(actor, preferredPoint, reserved = []) {
  const candidates = [preferredPoint];
  for (let ring = 1; ring <= 4; ring += 1) {
    const radius = ring * FORMATION_SPACING * 0.55;
    const slots = ring * 8;
    for (let index = 0; index < slots; index += 1) {
      const angle = (index / slots) * Math.PI * 2;
      candidates.push({
        x: preferredPoint.x + Math.cos(angle) * radius,
        y: preferredPoint.y + Math.sin(angle) * radius,
      });
    }
  }
  for (const point of candidates) {
    if (isReservedFormationPoint(point, reserved)) continue;
    const tile = tileAtMapPoint(point);
    if (!tile) continue;
    if (isValidMove(actor, tile.x, tile.y, undefined, point)) {
      reserved.push(point);
      return { x: tile.x, y: tile.y, worldX: point.x, worldY: point.y };
    }
  }
  return null;
}

function queueGroupMove(units, x, y, pointer = null) {
  const movable = units.filter(
    (unit) =>
      unit.owner === localOwner &&
      UNIT_TYPES[unit.type]?.airUnit !== true &&
      canQueueUnitOrder(unit, "move"),
  );
  if (movable.length < 2) return false;
  const totalCost = movable.reduce(
    (sum, unit) => sum + (UNIT_TYPES[unit.type]?.actionCost ?? 0),
    0,
  );
  if (state.players[localOwner].resources < totalCost) return false;

  const basePoint = targetPointForTile(x, y, pointer);
  const offsets = formationOffsets(movable.length);
  const reserved = [];
  const destinations = movable
    .map((unit, index) => ({
      unit,
      destination: findFormationDestination(
        unit,
        {
          x: basePoint.x + offsets[index].x,
          y: basePoint.y + offsets[index].y,
        },
        reserved,
      ),
    }))
    .filter((item) => item.destination);
  if (!destinations.length) return false;

  let queued = 0;
  for (const { unit, destination } of destinations) {
    if (
      queueOrder(unit, "move", destination.x, destination.y, localOwner, {
        worldX: destination.worldX,
        worldY: destination.worldY,
        preserveIntent: true,
        suppressLog: true,
      })
    )
      queued += 1;
  }
  if (!queued) return false;
  state.intent = null;
  log(`${queued} unit platoon route plotted to ${coord(x, y)}.`);
  schedulePlanSync();
  return true;
}

function handleTileClick(x, y, pointer = null) {
  if (isInteractionLocked()) return;
  const clickedStructure = state.structures.find((structure) =>
    occupiesTile(structure, x, y),
  );
  const clickedFriendlyUnit = nearestFriendlyUnitAt(x, y, pointer);
  if (isRealtimeGame()) {
    handleRealtimeTileClick(x, y, pointer, {
      clickedStructure,
      clickedFriendlyUnit,
    });
    return;
  }
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
    } else if (clickedFriendlyUnit) {
      selectUnit(clickedFriendlyUnit, { pointer });
      render();
      return;
    } else if (clickedStructure) {
      selectStructure(clickedStructure, pointer);
      render();
      return;
    } else closeStructurePopover();
    render();
    return;
  }

  if (state.phase !== "orders" && state.phase !== "attack") return;

  const lockedTarget = pointer?.targetId
    ? [...state.units, ...state.structures].find(
        (piece) =>
          piece.id === pointer.targetId &&
          piece.owner === enemyOwner() &&
          piece.hp > 0,
      )
    : null;
  const attackX = lockedTarget?.x ?? x;
  const attackY = lockedTarget?.y ?? y;
  const clicked = lockedTarget ?? pieceAt(x, y, false);
  const friendlyUnit = clickedFriendlyUnit;
  if (friendlyUnit) {
    selectUnit(friendlyUnit, {
      additive: Boolean(pointer?.shiftKey),
      pointer,
    });
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
  const destinationPoint = targetPointForTile(x, y, pointer);
  const firePoint = lockedTarget
    ? mapPositionFor(lockedTarget)
    : destinationPoint;
  const fireTarget = {
    x: attackX,
    y: attackY,
    worldX: firePoint.x,
    worldY: firePoint.y,
  };
  const group = selectedUnits();
  if (
    state.intent === "move" &&
    group.length > 1 &&
    group.every((unit) => UNIT_TYPES[unit.type]?.airUnit !== true)
  )
    orderQueued = queueGroupMove(group, x, y, pointer);
  else if (
    state.intent === "move" &&
    isValidMove(selected, x, y, undefined, destinationPoint)
  )
    orderQueued = queueOrder(selected, "move", x, y, localOwner, {
      worldX: destinationPoint.x,
      worldY: destinationPoint.y,
    });
  else if (
    state.intent === "attack" &&
    isValidTarget(selected, attackX, attackY, lockedTarget ?? fireTarget)
  )
    orderQueued = queueOrder(selected, "attack", attackX, attackY, localOwner, {
      targetId: lockedTarget?.id,
      worldX: firePoint.x,
      worldY: firePoint.y,
      preciseTarget: true,
    });
  else if (
    state.phase === "orders" &&
    UNIT_TYPES[selected.type]?.airUnit === true &&
    clicked?.owner === enemyOwner() &&
    isValidMove(selected, x, y, undefined, destinationPoint)
  )
    orderQueued = queueOrder(selected, "move", x, y, localOwner, {
      worldX: destinationPoint.x,
      worldY: destinationPoint.y,
    });
  else if (
    clicked &&
    clicked.owner === enemyOwner() &&
    isVisibleToPlayer(x, y) &&
    isValidTarget(selected, attackX, attackY, lockedTarget ?? fireTarget)
  ) {
    state.intent = "attack";
    orderQueued = queueOrder(selected, "attack", attackX, attackY, localOwner, {
      targetId: lockedTarget?.id,
      worldX: firePoint.x,
      worldY: firePoint.y,
      preciseTarget: true,
    });
  } else if (clickedStructure) {
    selectStructure(clickedStructure, pointer);
    render();
    return;
  }
  if (orderQueued) closeStructurePopover();
  render();
}

function handleRealtimeTileClick(
  x,
  y,
  pointer,
  { clickedStructure, clickedFriendlyUnit },
) {
  const destination = targetPointForTile(x, y, pointer);
  const clickedTarget = pointer?.targetId
    ? [...state.units, ...state.structures].find(
        (entity) => entity.id === pointer.targetId && entity.hp > 0,
      )
    : pieceAt(x, y, false);
  const commandClick = pointer?.button === 2;

  if (!commandClick && state.buildMode) {
    if (state.buildMode === "bridge") {
      if (!state.bridgeStart) {
        const commander = commanderForBridgeStart(x, y, localOwner);
        if (commander)
          state.bridgeStart = { x, y, commanderId: commander.id };
      } else if (queueBridge(state.bridgeStart, { x, y })) {
        state.buildMode = null;
        state.bridgeStart = null;
      }
    } else if (queueBuilding(state.buildMode, x, y)) {
      state.buildMode = null;
    }
    render();
    return;
  }

  if (!commandClick && clickedFriendlyUnit && !state.intent) {
    selectUnit(clickedFriendlyUnit, {
      additive: Boolean(pointer?.shiftKey),
      pointer,
    });
    render();
    return;
  }

  const selected = selectedUnits();
  if (selected.length && (commandClick || state.intent)) {
    const hostile =
      clickedTarget && clickedTarget.owner === enemyOwner()
        ? clickedTarget
        : null;
    if (hostile) {
      issueRtsCommand("attack", null, hostile.id, {
        queued: Boolean(pointer?.shiftKey),
        units: selected,
      });
      return;
    }
    const commandType =
      state.intent === "attack-move" ? "attack-move" : "move";
    issueFormationRtsCommand(commandType, destination, {
      queued: Boolean(pointer?.shiftKey),
      units: selected,
    });
    return;
  }

  if (!commandClick && clickedStructure) {
    selectStructure(clickedStructure, pointer);
  } else if (!commandClick) {
    clearUnitSelection();
    closeStructurePopover();
  }
  render();
}

function issueFormationRtsCommand(
  type,
  destination,
  { queued = false, units = selectedUnits() } = {},
) {
  if (!units.length) return false;
  const offsets = formationOffsets(units.length);
  let accepted = 0;
  const runtime = ensureRtsRuntime();
  const networkCommands = [];
  for (let index = 0; index < units.length; index += 1) {
    const offset = offsets[index];
    const target = {
      x: destination.x + offset.x,
      y: destination.y + offset.y,
    };
    const unit = units[index];
    const acceptedIds = runtime.issueCommand({
      owner: localOwner,
      unitIds: [unit.id],
      type,
      destination: target,
      queued,
    });
    if (acceptedIds.length) {
      accepted += 1;
      if (isHumanGame())
        networkCommands.push({
          commandType: type,
          destination: target,
          targetId: null,
          unitIds: acceptedIds,
          queued,
        });
    }
  }
  if (accepted) {
    if (networkCommands.length)
      sendSocketMessage({
        type: "rts-command-batch",
        sequence: ++multiplayer.sequence,
        commands: networkCommands,
      });
    state.intent = null;
    log(`${accepted} unit${accepted === 1 ? "" : "s"} executing ${type.replace("-", " ")}.`);
    render();
  }
  return accepted > 0;
}

function setIntent(intent) {
  if (isRealtimeGame()) {
    const normalized = intent === "attack" ? "attack-move" : intent;
    if (!selectedUnits().length || isInteractionLocked()) return;
    state.intent = state.intent === normalized ? null : normalized;
    render();
    return;
  }
  const selected = getUnit(state.selectedId);
  const group = selectedUnits();
  if (intent === "move" && group.length > 1) {
    const totalMoveCost = group.reduce(
      (sum, unit) => sum + (UNIT_TYPES[unit.type]?.actionCost ?? 0),
      0,
    );
    if (
      state.phase !== "orders" ||
      isInteractionLocked() ||
      group.some((unit) => UNIT_TYPES[unit.type]?.airUnit === true) ||
      group.some((unit) => !canQueueUnitOrder(unit, "move")) ||
      state.players[localOwner].resources < totalMoveCost
    )
      return;
    state.intent = state.intent === intent ? null : intent;
    render();
    return;
  }
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
  if (isRealtimeGame()) return null;
  const spec = UNIT_TYPES[unitItem?.type];
  return state.phase === "orders" &&
    spec?.airUnit === true &&
    unitItem.airborne !== true &&
    canQueueUnitOrder(unitItem, "move") &&
    state.players[localOwner].resources >= spec.actionCost
    ? "move"
    : null;
}

function isValidMove(actor, x, y, terrainPlan = undefined, targetPoint = null) {
  const isAirUnit = UNIT_TYPES[actor.type]?.airUnit === true;
  const destinationPoint = targetPoint ?? tileCenter(x, y);
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
    (isAirUnit ||
      !isGroundPositionOccupied(
        x,
        y,
        destinationPoint.x,
        destinationPoint.y,
        actor.id,
      )) &&
    (isAirUnit || Number.isFinite(groundMovementCostAt(x, y))) &&
    distance(actor, {
      x,
      y,
      worldX: destinationPoint.x,
      worldY: destinationPoint.y,
    }) > 0 &&
    (!isAirUnit ||
      distance(actor, {
        x,
        y,
        worldX: destinationPoint.x,
        worldY: destinationPoint.y,
      }) <= movementRangeFor(actor)) &&
    navigationGraph.isConnected(actor, { x, y }) &&
    (isAirUnit ||
      Boolean(
        terrainPlan !== undefined
          ? terrainPlan
          : movementRoutePlan(actor, x, y, destinationPoint),
      ))
  );
}

function isValidTarget(actor, x, y, target = null) {
  const destination = target ?? { x, y };
  return (
    canUnitAttack(actor) &&
    distance(actor, destination) > 0 &&
    distance(actor, destination) <= UNIT_TYPES[actor.type].range
  );
}

function queueOrder(actor, kind, x, y, owner = localOwner, options = {}) {
  if (owner && typeof owner === "object") {
    options = owner;
    owner = localOwner;
  }
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
  const destinationPoint =
    Number.isFinite(options.worldX) && Number.isFinite(options.worldY)
      ? { x: options.worldX, y: options.worldY }
      : tileCenter(x, y);
  const movementPlan =
    kind === "move" && !isAirUnit
      ? movementRoutePlan(actor, x, y, destinationPoint)
      : null;
  if (
    kind === "move" &&
    !isValidMove(actor, x, y, movementPlan, destinationPoint)
  )
    return false;
  const lockedTarget = options.targetId
    ? [...state.units, ...state.structures].find(
        (piece) => piece.id === options.targetId && piece.hp > 0,
      )
    : null;
  const preciseAttackTarget =
    kind === "attack" &&
    Number.isFinite(options.worldX) &&
    Number.isFinite(options.worldY)
      ? {
          x,
          y,
          worldX: options.worldX,
          worldY: options.worldY,
        }
      : null;
  if (
    kind === "attack" &&
    !isValidTarget(actor, x, y, lockedTarget ?? preciseAttackTarget)
  )
    return false;
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
    ...(kind === "move" || kind === "attack"
      ? { worldX: destinationPoint.x, worldY: destinationPoint.y }
      : {}),
    ...(kind === "attack" && lockedTarget
      ? { targetId: lockedTarget.id }
      : {}),
    ...(kind === "attack" && options.preciseTarget
      ? { preciseTarget: true }
      : {}),
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
  if (owner === localOwner && !options.suppressLog) {
    log(
      isAirUnit && kind === "move"
        ? `${UNIT_TYPES[actor.type].label} bombing run designated over ${coord(x, y)}.`
        : kind === "move" && movementPlan.waypoints.length > 1
          ? `${UNIT_TYPES[actor.type].label} route plotted to ${coord(x, y)} across ${movementPlan.waypoints.length} turns at ◆ ${cost} per turn moved.`
          : `${UNIT_TYPES[actor.type].label} unit ordered to ${kind === "move" ? "advance to" : "fire on"} ${coord(x, y)}.`,
    );
    schedulePlanSync();
  }
  if (!options.preserveIntent) state.intent = null;
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
  if (item.kind === "command") {
    const actor = getUnit(item.unitId);
    if (actor) {
      actor.path = [];
      actor.pathIndex = 0;
      actor.velocityX = 0;
      actor.velocityY = 0;
      actor.explicitTargetId = null;
      actor.acquiredTargetId = null;
    }
    log("Live command cancelled.");
    render();
    return;
  }
  const refundable =
    item.kind !== "move" ||
    !Number.isInteger(item.waypointIndex) ||
    item.waypointIndex === 0;
  if (refundable)
    state.players[localOwner].resources += Number(item.cost ?? 0);
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

async function fastForwardConfiguredTurns() {
  if (isHumanGame() || fastForwarding || state.resolving || state.winner) return;
  const turns = Math.max(
    1,
    Math.min(10, Number.parseInt(els.turnSkipCount?.value ?? "1", 10) || 1),
  );
  closeStructurePopover();
  clearUnitSelection();
  state.buildMode = null;
  state.bridgeStart = null;
  fastForwarding = true;
  log(`Strategic time advance initiated for ${turns} round${turns === 1 ? "" : "s"}.`);
  render();

  let completed = 0;
  try {
    while (completed < turns && !state.winner) {
      if (state.phase === "deployment") state.phase = "orders";
      if (state.phase === "orders")
        await phaseController.beginAttackPhase({ includeAi: true, present: false });
      if (state.phase === "attack") {
        await phaseController.resolveRound({ includeAi: true, present: false });
        completed += 1;
      }
    }
  } finally {
    fastForwarding = false;
  }
  log(
    `Strategic time advance complete. ${completed} round${completed === 1 ? "" : "s"} resolved.`,
  );
  render();
}

function checkWinner() {
  if (state.objectiveWinner === 0 || state.objectiveWinner === 1) {
    state.winner = state.objectiveWinner;
    state.resolving = false;
    showWinnerResult();
    return;
  }
  const playerAlive =
    state.units.some((item) => item.owner === 0) ||
    playerBarracks(0).length > 0;
  const enemyAlive =
    state.units.some((item) => item.owner === 1) ||
    playerBarracks(1).length > 0;
  if (playerAlive && enemyAlive) return;
  state.winner = playerAlive ? 0 : enemyAlive ? 1 : "draw";
  if (activeWorldSectorId && worldState && state.winner !== "draw") {
    const sector = worldState.sectors.find(
      (item) => item.id === activeWorldSectorId,
    );
    if (sector) {
      sector.modifications =
        terrainSampler?.serializeModifications?.() ?? sector.modifications;
      applySectorOutcome(worldState, activeWorldSectorId, state.winner);
      worldState.turn += 1;
      saveWorldCampaign();
    }
    activeWorldSectorId = null;
  }
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

function initializeBattlefield3D({
  deferVegetation = false,
  deferFogClouds = false,
  viewState = null,
} = {}) {
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
      getWorldPoint: mapPositionFor,
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
      terrainFlatteningAreas: terrainFlatteningAreas(),
      terrainDebug: gameConfig.terrainDebug,
      showElevationValues: gameConfig.terrainDebugElevation,
      deferVegetation,
      deferFogClouds,
      authoredTreeDensity: getMapPreset(selectedMapId).authoredTerrain
        ?.treeDensity,
      resolvePlacementPreview: (x, y) => {
        const type = state.buildMode;
        const spec = BUILDINGS[type];
        if (
          !type ||
          !spec ||
          !["deployment", "realtime"].includes(state.phase)
        )
          return null;
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
          ...(Number.isFinite(spec.modelWidth)
            ? { modelWidth: spec.modelWidth }
            : {}),
          ...(Number.isFinite(spec.modelDepth)
            ? { modelDepth: spec.modelDepth }
            : {}),
        };
      },
      onPlacementPreviewChange: updateBridgePlacementReadout,
      onBoxSelect: (unitIds) => {
        if (isInteractionLocked() || !unitIds.length) return;
        closeStructurePopover();
        selectedStructureId = null;
        state.selectedIds = unitIds.filter((id) => {
          const unit = getUnit(id);
          return unit?.owner === localOwner && unit.hp > 0;
        });
        state.selectedId = state.selectedIds[0] ?? null;
        state.intent = defaultSelectionIntentForUnits(selectedUnits());
        render();
      },
      onTileClick: (x, y, pointer) => {
        if (isInteractionLocked()) return;
        handleTileClick(x, y, pointer);
      },
      onViewportChange: () => {
        if (!battlefield3D || !battlefieldView) return;
        renderMap(battlefieldView, getUnit(state.selectedId));
      },
    });
    battlefield3D.restoreViewState(viewState);
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
customMapsReady
  .then((maps) => {
    customMapLibrary = maps;
    registerCustomMaps(maps);
    renderMapOptions();
    renderMapBuilderLibrary();
  })
  .catch((error) => console.warn("Custom maps could not be loaded.", error));
document.body.classList.toggle(
  "reduced-motion",
  Boolean(readUiSettings().reducedMotion),
);
showGameMenu("home");
render();
requestAnimationFrame(rtsFrame);
