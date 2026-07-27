// CommonJS entrypoint; browser and shared simulation modules use ESM.
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { WebSocket, WebSocketServer } = require('ws');
const { createCampaignRepository } = require('./server/campaign-repository.cjs');

const root = __dirname;
const host = process.env.HOST || '0.0.0.0';
const port = Number(process.env.PORT) || 4173;
const websocketPath = '/ws';
const legacyMatchStorePath = process.env.MATCH_STORE_PATH
  ? path.resolve(process.env.MATCH_STORE_PATH)
  : path.join(root, '.data', 'matches.json');
const databasePath = process.env.DATABASE_PATH
  ? path.resolve(process.env.DATABASE_PATH)
  : path.join(root, '.data', 'rule-of-war.sqlite');
const campaignRepository = createCampaignRepository(databasePath);
const maxMessageBytes = 1024 * 1024;
const matchCodeAlphabet = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';
const matches = new Map();
const liveSockets = new Map();
let validatePlayerPlan = null;
let serverUnitTypes = null;
let serverBuildings = null;
let RtsRuntime = null;
let createServerUnit = null;
let createServerStructure = null;
const rtsRuntimes = new Map();

const mime = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.terrain': 'application/octet-stream',
};
const publicRootFiles = new Set([
  'index.html',
  'styles.css',
  'game.js',
  'game-rules.js',
  'battlefield3d.js',
  'map-builder3d.js',
  'model-gallery.html',
  'model-gallery.css',
  'model-gallery.js',
  'favicon.svg',
  'game-home.png',
]);
const publicDirectories = new Set([
  'assets',
  'config',
  'controllers',
  'core',
  'maps',
  'network',
  'simulation',
  'state',
  'storage',
  'terrain',
  'ui',
]);

function isRecord(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function cloneJson(value) {
  return JSON.parse(JSON.stringify(value));
}

function normalizeName(value, fallback) {
  if (typeof value !== 'string') return fallback;
  const cleaned = value.trim().replace(/\s+/g, ' ').slice(0, 32);
  return cleaned || fallback;
}

function normalizeCode(value) {
  return typeof value === 'string'
    ? value.toUpperCase().replace(/[^A-Z0-9]/g, '').slice(0, 6)
    : '';
}

function assertSnapshot(value) {
  if (
    !isRecord(value) ||
    !isRecord(value.config) ||
    !isRecord(value.game) ||
    !Array.isArray(value.game.players) ||
    value.game.players.length < 2 ||
    !Array.isArray(value.game.jobs) ||
    !Array.isArray(value.game.orders)
  ) {
    throw new ProtocolError(
      'INVALID_STATE',
      'The game snapshot is incomplete or invalid.',
    );
  }

  return cloneJson(value);
}

function canonicalJson(value) {
  if (Array.isArray(value)) {
    return `[${value.map((item) => canonicalJson(item)).join(',')}]`;
  }

  if (isRecord(value)) {
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
      .join(',')}}`;
  }

  return JSON.stringify(value);
}

function hashSnapshot(snapshot) {
  return crypto.createHash('sha256').update(canonicalJson(snapshot)).digest('hex');
}

function createToken() {
  return crypto.randomBytes(24).toString('base64url');
}

function createMatchCode() {
  for (let attempt = 0; attempt < 1000; attempt += 1) {
    let code = '';
    const bytes = crypto.randomBytes(6);
    for (const byte of bytes) {
      code += matchCodeAlphabet[byte % matchCodeAlphabet.length];
    }
    if (!matches.has(code)) return code;
  }
  throw new ProtocolError('SERVER_BUSY', 'Could not allocate a match code.');
}

function serializableMatch(match) {
  return {
    code: match.code,
    createdAt: match.createdAt,
    updatedAt: match.updatedAt,
    players: match.players,
    state: match.state,
    version: match.version,
    revision: match.revision,
    sequence: match.sequence,
    ready: match.ready,
    pendingAdvance: match.pendingAdvance,
    completions: match.completions,
    lastCheckpoint: match.lastCheckpoint,
  };
}

function persistMatches() {
  try {
    campaignRepository.replaceMatches(
      [...matches.values()].map(serializableMatch),
    );
  } catch (error) {
    console.error(`Could not persist multiplayer matches: ${error.message}`);
  }
}

function normalizeLoadedPlayer(player, index) {
  if (!isRecord(player) || typeof player.token !== 'string') return null;
  return {
    token: player.token,
    name: normalizeName(player.name, index === 0 ? 'Field Command' : 'Iron Front'),
    joinedAt: typeof player.joinedAt === 'string' ? player.joinedAt : new Date().toISOString(),
    lastSeen: typeof player.lastSeen === 'string' ? player.lastSeen : null,
  };
}

function normalizeLoadedCompletion(completion) {
  if (!isRecord(completion)) return null;
  try {
    const state = assertSnapshot(completion.state);
    return {
      state,
      hash: hashSnapshot(state),
      reportedHash:
        typeof completion.reportedHash === 'string'
          ? completion.reportedHash.slice(0, 128)
          : null,
      receivedAt:
        typeof completion.receivedAt === 'string'
          ? completion.receivedAt
          : new Date().toISOString(),
    };
  } catch {
    return null;
  }
}

function normalizeLoadedMatch(raw) {
  if (!isRecord(raw)) return null;
  const code = normalizeCode(raw.code);
  if (code.length !== 6) return null;

  let state;
  try {
    state = assertSnapshot(raw.state);
  } catch {
    return null;
  }

  const sequence = Number.isSafeInteger(raw.sequence) && raw.sequence >= 0 ? raw.sequence : 0;
  let pendingAdvance = null;
  if (
    isRecord(raw.pendingAdvance) &&
    raw.pendingAdvance.sequence === sequence &&
    Number.isInteger(raw.pendingAdvance.seed)
  ) {
    try {
      pendingAdvance = {
        sequence,
        phase:
          typeof raw.pendingAdvance.phase === 'string'
            ? raw.pendingAdvance.phase
            : state.game.phase,
        seed: raw.pendingAdvance.seed >>> 0,
        state: assertSnapshot(raw.pendingAdvance.state || state),
        startedAt:
          typeof raw.pendingAdvance.startedAt === 'string'
            ? raw.pendingAdvance.startedAt
            : new Date().toISOString(),
      };
    } catch {
      pendingAdvance = null;
    }
  }

  const completions = pendingAdvance
    ? [
        normalizeLoadedCompletion(raw.completions?.[0]),
        normalizeLoadedCompletion(raw.completions?.[1]),
      ]
    : [null, null];

  const match = {
    code,
    createdAt: typeof raw.createdAt === 'string' ? raw.createdAt : new Date().toISOString(),
    updatedAt: typeof raw.updatedAt === 'string' ? raw.updatedAt : new Date().toISOString(),
    players: [normalizeLoadedPlayer(raw.players?.[0], 0), normalizeLoadedPlayer(raw.players?.[1], 1)],
    state,
    version: Number.isSafeInteger(raw.version) && raw.version > 0 ? raw.version : 1,
    revision: Number.isSafeInteger(raw.revision) && raw.revision >= 0 ? raw.revision : 0,
    sequence,
    ready: pendingAdvance
      ? [false, false]
      : [Boolean(raw.ready?.[0]), Boolean(raw.ready?.[1])],
    pendingAdvance,
    completions,
    lastCheckpoint: isRecord(raw.lastCheckpoint) ? cloneJson(raw.lastCheckpoint) : null,
  };

  if (!match.players[0]) return null;

  if (pendingAdvance && completions[0] && completions[1]) {
    const hashesMatch = completions[0].hash === completions[1].hash;
    match.state = hashesMatch
      ? completions[0].state
      : cloneJson(pendingAdvance.state);
    match.version += 1;
    match.revision += 1;
    match.pendingAdvance = null;
    match.completions = [null, null];
    match.lastCheckpoint = {
      sequence,
      hash: hashSnapshot(match.state),
      hashesMatch,
      rejected: !hashesMatch,
      version: match.version,
      revision: match.revision,
    };
  }

  return match;
}

function loadMatches() {
  try {
    let storedMatches = campaignRepository.loadMatches();
    if (!storedMatches.length && fs.existsSync(legacyMatchStorePath)) {
      const legacy = JSON.parse(fs.readFileSync(legacyMatchStorePath, 'utf8'));
      storedMatches = Array.isArray(legacy.matches) ? legacy.matches : [];
    }
    for (const raw of storedMatches) {
      const match = normalizeLoadedMatch(raw);
      if (match) matches.set(match.code, match);
    }
    if (matches.size) persistMatches();
    console.log(`Loaded ${matches.size} saved multiplayer match${matches.size === 1 ? '' : 'es'}.`);
  } catch (error) {
    console.error(`Could not load saved multiplayer matches: ${error.message}`);
  }
}

class ProtocolError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

function send(socket, message) {
  if (socket.readyState === WebSocket.OPEN) {
    socket.send(JSON.stringify(message));
  }
}

function sendError(socket, code, message) {
  send(socket, { type: 'error', code, message });
}

function socketsFor(match) {
  if (!liveSockets.has(match.code)) liveSockets.set(match.code, [null, null]);
  return liveSockets.get(match.code);
}

function connectedPlayers(match) {
  const sockets = socketsFor(match);
  return [0, 1].map(
    (index) => Boolean(match.players[index]) && sockets[index]?.readyState === WebSocket.OPEN,
  );
}

function publicPlayers(match) {
  return match.players.map((player, index) => ({
    name: player?.name || (index === 0 ? 'Field Command' : 'Iron Front'),
    joined: Boolean(player),
  }));
}

function publicPendingAdvance(match) {
  if (!match.pendingAdvance) return null;
  return {
    sequence: match.pendingAdvance.sequence,
    phase: match.pendingAdvance.phase,
    seed: match.pendingAdvance.seed,
    state: match.pendingAdvance.state,
  };
}

function matchStatus(match) {
  return {
    type: 'match-status',
    code: match.code,
    connected: connectedPlayers(match),
    ready: [...match.ready],
    players: publicPlayers(match),
    pendingAdvance: publicPendingAdvance(match),
    completed: match.completions.map(Boolean),
    version: match.version,
    revision: match.revision,
    sequence: match.sequence,
  };
}

function sessionPayload(match, playerIndex, action, type = 'session') {
  return {
    type,
    ...(action ? { action } : {}),
    code: match.code,
    token: match.players[playerIndex].token,
    playerIndex,
    state: match.state,
    version: match.version,
    revision: match.revision,
    sequence: match.sequence,
    ready: [...match.ready],
    connected: connectedPlayers(match),
    players: publicPlayers(match),
    pendingAdvance: publicPendingAdvance(match),
    completed: match.completions.map(Boolean),
  };
}

function broadcast(match, message, except = null) {
  for (const socket of socketsFor(match)) {
    if (socket && socket !== except) send(socket, message);
  }
}

function broadcastStatus(match) {
  broadcast(match, matchStatus(match));
}

function ensureUnbound(socket) {
  if (socket.matchCode) {
    throw new ProtocolError('ALREADY_IN_MATCH', 'This connection is already in a match.');
  }
}

function requireSession(socket) {
  if (!socket.matchCode || !Number.isInteger(socket.playerIndex)) {
    throw new ProtocolError('NOT_IN_MATCH', 'Join or resume a match first.');
  }
  const match = matches.get(socket.matchCode);
  if (!match) throw new ProtocolError('MATCH_NOT_FOUND', 'That match no longer exists.');
  return { match, playerIndex: socket.playerIndex };
}

function bindSocket(socket, match, playerIndex) {
  const sockets = socketsFor(match);
  const previous = sockets[playerIndex];
  sockets[playerIndex] = socket;
  socket.matchCode = match.code;
  socket.playerIndex = playerIndex;
  match.players[playerIndex].lastSeen = new Date().toISOString();

  if (previous && previous !== socket && previous.readyState === WebSocket.OPEN) {
    send(previous, {
      type: 'session-replaced',
      message: 'This player slot was resumed in another browser.',
    });
    previous.close(4001, 'Session resumed elsewhere');
  }
}

function createMatch(socket, message) {
  ensureUnbound(socket);
  if (matches.size >= 1000) {
    throw new ProtocolError('SERVER_BUSY', 'The match server is at capacity.');
  }

  const state = assertSnapshot(message.state);
  const now = new Date().toISOString();
  const code = createMatchCode();
  const match = {
    code,
    createdAt: now,
    updatedAt: now,
    players: [
      {
        token: createToken(),
        name: normalizeName(message.name, 'Field Command'),
        joinedAt: now,
        lastSeen: now,
      },
      null,
    ],
    state,
    version: 1,
    revision: 0,
    sequence: 0,
    ready: [false, false],
    pendingAdvance: null,
    completions: [null, null],
    lastCheckpoint: null,
  };

  matches.set(code, match);
  bindSocket(socket, match, 0);
  persistMatches();
  send(socket, sessionPayload(match, 0, 'created'));
  broadcastStatus(match);
}

function joinMatch(socket, message) {
  ensureUnbound(socket);
  const code = normalizeCode(message.code);
  const match = matches.get(code);
  if (!match) throw new ProtocolError('MATCH_NOT_FOUND', 'No saved match uses that code.');
  if (match.players[1]) {
    throw new ProtocolError('MATCH_FULL', 'That match already has two players. Use Resume instead.');
  }
  if (match.pendingAdvance) {
    throw new ProtocolError('MATCH_IN_PROGRESS', 'That match is already resolving a phase.');
  }

  const now = new Date().toISOString();
  match.players[1] = {
    token: createToken(),
    name: normalizeName(message.name, 'Iron Front'),
    joinedAt: now,
    lastSeen: now,
  };
  match.updatedAt = now;
  bindSocket(socket, match, 1);
  persistMatches();
  send(socket, sessionPayload(match, 1, 'joined'));
  broadcastStatus(match);
}

function resumeMatch(socket, message) {
  ensureUnbound(socket);
  const code = normalizeCode(message.code);
  const match = matches.get(code);
  if (!match) throw new ProtocolError('MATCH_NOT_FOUND', 'No saved match uses that code.');
  if (typeof message.token !== 'string') {
    throw new ProtocolError('INVALID_TOKEN', 'A resume token is required.');
  }
  const playerIndex = match.players.findIndex((player) => player?.token === message.token);
  if (playerIndex < 0) {
    throw new ProtocolError('INVALID_TOKEN', 'That resume token is not valid for this match.');
  }

  bindSocket(socket, match, playerIndex);
  match.updatedAt = new Date().toISOString();
  persistMatches();
  send(socket, sessionPayload(match, playerIndex, 'resumed'));
  broadcastStatus(match);
  maybeStartPhaseAdvance(match);
}

function sanitizeOwnedItems(items, playerIndex, label) {
  if (!Array.isArray(items) || items.length > 10000) {
    throw new ProtocolError('INVALID_PLAN', `${label} must be a valid array.`);
  }

  return items.map((item) => {
    if (!isRecord(item)) {
      throw new ProtocolError('INVALID_PLAN', `${label} contains an invalid item.`);
    }
    return { ...cloneJson(item), owner: playerIndex };
  });
}

function replaceOwnedItems(current, replacement, playerIndex) {
  const otherItems = Array.isArray(current)
    ? current.filter((item) => !isRecord(item) || item.owner !== playerIndex)
    : [];
  return [...otherItems, ...replacement];
}

function updatePlan(socket, message) {
  const { match, playerIndex } = requireSession(socket);
  if (match.pendingAdvance) {
    throw new ProtocolError('PHASE_LOCKED', 'This phase is currently resolving.');
  }
  if (match.ready[playerIndex]) {
    throw new ProtocolError('PLAYER_READY', 'Unready before changing your plan.');
  }
  const game = match.state.game;
  if (!isRecord(game.players[playerIndex])) {
    throw new ProtocolError('INVALID_STATE', 'The player record is missing from this match.');
  }
  let validated;
  try {
    validated = validatePlayerPlan({
      state: game,
      playerIndex,
      jobs: message.jobs,
      orders: message.orders,
      unitTypes: serverUnitTypes,
      buildings: serverBuildings,
      cols: match.state.config.cols,
      rows: match.state.config.rows,
    });
  } catch (error) {
    throw new ProtocolError(
      typeof error.code === 'string' ? error.code : 'INVALID_PLAN',
      error.message || 'The submitted plan is invalid.',
    );
  }
  const { jobs, orders, resources } = validated;

  game.jobs = replaceOwnedItems(game.jobs, jobs, playerIndex);
  game.orders = replaceOwnedItems(game.orders, orders, playerIndex);
  game.players[playerIndex] = {
    ...game.players[playerIndex],
    resources,
  };
  match.revision += 1;
  match.updatedAt = new Date().toISOString();
  persistMatches();
  broadcast(match, {
    type: 'state-update',
    source: 'plan',
    playerIndex,
    jobs,
    orders,
    resources,
    version: match.version,
    revision: match.revision,
  });
}

function startPhaseAdvance(match) {
  match.sequence += 1;
  const seed = crypto.randomBytes(4).readUInt32BE(0);
  match.pendingAdvance = {
    sequence: match.sequence,
    phase: match.state.game.phase,
    seed,
    state: cloneJson(match.state),
    startedAt: new Date().toISOString(),
  };
  match.ready = [false, false];
  match.completions = [null, null];
  match.updatedAt = new Date().toISOString();
  persistMatches();
  broadcast(match, {
    type: 'phase-advance',
    phase: match.pendingAdvance.phase,
    seed,
    state: match.pendingAdvance.state,
    sequence: match.sequence,
  });
  broadcastStatus(match);
}

function setReady(socket, message) {
  const { match, playerIndex } = requireSession(socket);
  if (match.pendingAdvance) {
    throw new ProtocolError('PHASE_LOCKED', 'This phase is currently resolving.');
  }
  if (typeof message.ready !== 'boolean') {
    throw new ProtocolError('INVALID_READY', 'Ready must be true or false.');
  }

  match.ready[playerIndex] = message.ready;
  match.updatedAt = new Date().toISOString();
  persistMatches();
  broadcast(match, {
    type: 'ready-update',
    ready: [...match.ready],
    connected: connectedPlayers(match),
  });

  maybeStartPhaseAdvance(match);
}

function maybeStartPhaseAdvance(match) {
  if (
    !match.pendingAdvance &&
    match.players[1] &&
    match.ready.every(Boolean) &&
    connectedPlayers(match).every(Boolean)
  )
    startPhaseAdvance(match);
}

function checkpointPayload(match, checkpoint = match.lastCheckpoint) {
  return {
    type: 'checkpoint',
    state: match.state,
    hash: checkpoint?.hash || hashSnapshot(match.state),
    hashesMatch: checkpoint?.hashesMatch ?? true,
    rejected: checkpoint?.rejected ?? false,
    sequence: checkpoint?.sequence ?? match.sequence,
    version: match.version,
    revision: match.revision,
  };
}

function finalizePhaseCheckpoint(match, { broadcastResult = true } = {}) {
  if (!match.pendingAdvance || !match.completions.every(Boolean)) return false;
  const [hostCompletion, guestCompletion] = match.completions;
  const hashesMatch = hostCompletion.hash === guestCompletion.hash;
  match.state = hashesMatch
    ? hostCompletion.state
    : cloneJson(match.pendingAdvance.state);
  match.version += 1;
  match.revision += 1;
  match.pendingAdvance = null;
  match.completions = [null, null];
  match.ready = [false, false];
  match.updatedAt = new Date().toISOString();
  match.lastCheckpoint = {
    sequence: match.sequence,
    hash: hashSnapshot(match.state),
    hashesMatch,
    rejected: !hashesMatch,
    version: match.version,
    revision: match.revision,
  };
  persistMatches();
  if (broadcastResult) {
    broadcast(match, checkpointPayload(match));
    broadcastStatus(match);
  }
  return true;
}

function completePhase(socket, message) {
  const { match, playerIndex } = requireSession(socket);
  if (!match.pendingAdvance) {
    if (match.lastCheckpoint?.sequence === message.sequence) {
      send(socket, checkpointPayload(match));
      return;
    }
    throw new ProtocolError('NO_PHASE_PENDING', 'There is no phase waiting to be completed.');
  }
  if (message.sequence !== match.pendingAdvance.sequence) {
    throw new ProtocolError('STALE_SEQUENCE', 'That phase completion is no longer current.');
  }

  const state = assertSnapshot(message.state);
  const completion = {
    state,
    hash: hashSnapshot(state),
    reportedHash:
      typeof message.hash === 'string' ? message.hash.slice(0, 128) : null,
    receivedAt: new Date().toISOString(),
  };
  match.completions[playerIndex] = completion;
  match.updatedAt = new Date().toISOString();
  persistMatches();

  if (!match.completions.every(Boolean)) {
    broadcastStatus(match);
    return;
  }
  finalizePhaseCheckpoint(match);
}

function requestSync(socket) {
  const { match, playerIndex } = requireSession(socket);
  send(socket, sessionPayload(match, playerIndex, null, 'sync'));
}

function completeServerProduction(match, job) {
  const game = match.state.game;
  if (job.kind === 'building') {
    const structure = createServerStructure(
      job.type,
      job.owner,
      job.x,
      job.y,
      `s-${job.id}`,
      job.type !== 'bridge',
      Array.isArray(job.footprint) ? job.footprint : [{ x: job.x, y: job.y }],
    );
    if (job.type === 'bridge') {
      structure.endX = job.endX;
      structure.endY = job.endY;
      structure.lengthMeters = job.lengthMeters;
    }
    game.structures.push(structure);
    return;
  }
  if (job.kind !== 'recruit') return;
  const producer = game.structures.find(
    (structure) => structure.id === (job.producerId || job.barracksId),
  );
  if (!producer) {
    game.players[job.owner].resources += Number(job.cost) || 0;
    return;
  }
  const offset = game.units.filter(
    (unit) => Math.hypot(unit.x - producer.x, unit.y - producer.y) < 3,
  ).length;
  const angle = (offset % 8) * Math.PI / 4;
  const x = producer.x + Math.cos(angle) * 2;
  const y = producer.y + Math.sin(angle) * 2;
  const unit = createServerUnit(
    job.type,
    job.owner,
    Math.round(x),
    Math.round(y),
    `u-${job.id}`,
  );
  unit.worldX = x;
  unit.worldY = y;
  if (serverUnitTypes[job.type]?.airUnit) {
    unit.homeAirfieldId = producer.id;
    unit.parkingSlot = offset % Math.max(
      1,
      Number(serverBuildings.airfield?.aircraftCapacity ?? 3),
    );
    unit.worldX =
      producer.x + ([0.1, 1, 1.9][unit.parkingSlot] ?? 0);
    unit.worldY = producer.y + 0.8;
    unit.x = producer.x;
    unit.y = producer.y;
    unit.airborne = false;
    unit.airState = "landed";
    unit.maxFuel = Number(serverUnitTypes[job.type].fuelCapacity ?? 1);
    unit.fuel = unit.maxFuel;
  }
  game.units.push(unit);
}

function ensureMatchRuntime(match) {
  const existing = rtsRuntimes.get(match.code);
  if (existing?.state === match.state.game) return existing;
  let runtimeCounter = Number(match.state.game.nextRuntimeId) || 1;
  const runtime = new RtsRuntime({
    state: match.state.game,
    unitTypes: serverUnitTypes,
    buildingTypes: serverBuildings,
    tickRate: 20,
    seed: match.code,
    createId: (prefix) => `${prefix}-server-${runtimeCounter++}`,
    isPassable: (x, y) =>
      Number.isFinite(x) &&
      Number.isFinite(y) &&
      x >= 0 &&
      y >= 0 &&
      x < match.state.config.cols &&
      y < match.state.config.rows,
    isVisible: () => true,
    onEvent: (event) => {
      if (event.type === 'production-complete')
        completeServerProduction(match, event.job);
      broadcast(match, { type: 'rts-event', event });
    },
  });
  rtsRuntimes.set(match.code, runtime);
  return runtime;
}

function issueRtsCommand(socket, message) {
  const { match, playerIndex } = requireSession(socket);
  const game = match.state.game;
  if (game.phase !== 'realtime') {
    throw new ProtocolError('INVALID_MODE', 'This match is not running in real time.');
  }
  if (
    !Array.isArray(message.unitIds) ||
    !message.unitIds.length ||
    message.unitIds.length > 200
  ) {
    throw new ProtocolError('INVALID_COMMAND', 'A command requires 1 to 200 units.');
  }
  const unitIds = [...new Set(message.unitIds.map(String))];
  if (
    unitIds.some(
      (id) =>
        !game.units.some(
          (unit) => String(unit.id) === id && unit.owner === playerIndex && unit.hp > 0,
        ),
    )
  ) {
    throw new ProtocolError('INVALID_COMMAND', 'The command contains an unowned unit.');
  }
  const allowed = new Set([
    'move',
    'attack',
    'attack-move',
    'stop',
    'hold',
    'patrol',
    'attack-stance',
    'flee',
    'hold-fire',
  ]);
  const commandType = message.commandType;
  if (!allowed.has(commandType)) {
    throw new ProtocolError('INVALID_COMMAND', 'Unknown real-time command.');
  }
  let destination = null;
  if (
    ![
      'attack',
      'stop',
      'hold',
      'attack-stance',
      'flee',
      'hold-fire',
    ].includes(commandType)
  ) {
    const x = Number(message.destination?.x);
    const y = Number(message.destination?.y);
    if (
      !Number.isFinite(x) ||
      !Number.isFinite(y) ||
      x < 0 ||
      y < 0 ||
      x >= match.state.config.cols ||
      y >= match.state.config.rows
    ) {
      throw new ProtocolError('INVALID_COMMAND', 'The destination is outside the map.');
    }
    destination = { x, y };
  }
  let targetId = null;
  if (commandType === 'attack') {
    targetId = String(message.targetId || '');
    const target = [...game.units, ...game.structures].find(
      (entity) =>
        String(entity.id) === targetId &&
        entity.owner !== playerIndex &&
        entity.hp > 0,
    );
    if (!target) {
      throw new ProtocolError('INVALID_COMMAND', 'The attack target is not valid.');
    }
  }
  const accepted = ensureMatchRuntime(match).issueCommand({
    owner: playerIndex,
    unitIds,
    type: commandType,
    destination,
    targetId,
    queued: Boolean(message.queued),
  });
  match.revision += 1;
  match.updatedAt = new Date().toISOString();
  broadcast(match, {
    type: 'command-accepted',
    playerIndex,
    sequence: Number(message.sequence) || 0,
    unitIds: accepted,
  });
}

function issueRtsCommandBatch(socket, message) {
  if (!Array.isArray(message.commands) || message.commands.length > 200) {
    throw new ProtocolError('INVALID_COMMAND', 'A command batch may contain up to 200 commands.');
  }
  for (const command of message.commands)
    issueRtsCommand(socket, {
      ...command,
      sequence: message.sequence,
    });
}

function handleMessage(socket, rawMessage) {
  if (Date.now() - socket.rateWindowStartedAt > 1000) {
    socket.rateWindowStartedAt = Date.now();
    socket.rateWindowCount = 0;
  }
  socket.rateWindowCount += 1;
  if (socket.rateWindowCount > 60) {
    throw new ProtocolError('RATE_LIMIT', 'Too many messages were sent at once.');
  }

  let message;
  try {
    message = JSON.parse(rawMessage.toString());
  } catch {
    throw new ProtocolError('INVALID_MESSAGE', 'Messages must be valid JSON.');
  }
  if (!isRecord(message) || typeof message.type !== 'string') {
    throw new ProtocolError('INVALID_MESSAGE', 'A message type is required.');
  }

  switch (message.type) {
    case 'create-match':
      createMatch(socket, message);
      break;
    case 'join-match':
      joinMatch(socket, message);
      break;
    case 'resume-match':
      resumeMatch(socket, message);
      break;
    case 'plan-update':
      updatePlan(socket, message);
      break;
    case 'set-ready':
      setReady(socket, message);
      break;
    case 'phase-complete':
      completePhase(socket, message);
      break;
    case 'rts-command':
      issueRtsCommand(socket, message);
      break;
    case 'rts-command-batch':
      issueRtsCommandBatch(socket, message);
      break;
    case 'request-sync':
      requestSync(socket);
      break;
    case 'ping':
      send(socket, { type: 'pong', timestamp: Date.now() });
      break;
    default:
      throw new ProtocolError('UNKNOWN_MESSAGE', `Unknown message type: ${message.type}`);
  }
}

function handleDisconnect(socket) {
  if (!socket.matchCode || !Number.isInteger(socket.playerIndex)) return;
  const match = matches.get(socket.matchCode);
  if (!match) return;
  const sockets = socketsFor(match);
  if (sockets[socket.playerIndex] !== socket) return;

  sockets[socket.playerIndex] = null;
  match.players[socket.playerIndex].lastSeen = new Date().toISOString();
  match.updatedAt = new Date().toISOString();
  persistMatches();
  broadcastStatus(match);
}

function serveStatic(request, response) {
  let pathname;
  try {
    pathname = decodeURIComponent(new URL(request.url, 'http://localhost').pathname);
  } catch {
    response.writeHead(400).end('Bad request');
    return;
  }

  if (pathname === '/favicon.ico') {
    response.writeHead(204).end();
    return;
  }
  if (request.method !== 'GET' && request.method !== 'HEAD') {
    response.writeHead(405, { Allow: 'GET, HEAD' }).end('Method not allowed');
    return;
  }

  const requested = pathname === '/' ? 'index.html' : pathname.replace(/^\/+/, '');
  const requestedParts = requested.split('/').filter(Boolean);
  const containsPrivateSegment = requestedParts.some(
    (segment) => segment.startsWith('.'),
  );
  const allowedDependency =
    requestedParts[0] === 'node_modules' && requestedParts[1] === 'three';
  const allowedPublicPath =
    publicRootFiles.has(requested) ||
    publicDirectories.has(requestedParts[0]) ||
    allowedDependency;
  if (containsPrivateSegment || !allowedPublicPath) {
    response.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' }).end('Not found');
    return;
  }

  const filePath = path.resolve(root, requested);
  const relative = path.relative(root, filePath);
  if (relative.startsWith('..') || path.isAbsolute(relative)) {
    response.writeHead(403).end('Forbidden');
    return;
  }

  const acceptsGzip = /\bgzip\b/.test(request.headers['accept-encoding'] || '');
  const compressedPath =
    acceptsGzip && path.extname(filePath) === '.terrain'
      ? `${filePath}.gz`
      : null;
  const responsePath =
    compressedPath && fs.existsSync(compressedPath) ? compressedPath : filePath;
  fs.readFile(responsePath, (error, data) => {
    if (error) {
      response.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' }).end('Not found');
      return;
    }
    response.writeHead(200, {
      'Content-Type': mime[path.extname(filePath)] || 'application/octet-stream',
      'Cache-Control': 'no-store',
      Vary: 'Accept-Encoding',
      ...(responsePath === compressedPath ? { 'Content-Encoding': 'gzip' } : {}),
    });
    if (request.method === 'HEAD') response.end();
    else response.end(data);
  });
}

async function startServer() {
  const [validationModule, rulesModule, runtimeModule, stateModule] = await Promise.all([
    import('./simulation/command-validation.js'),
    import('./game-rules.js'),
    import('./simulation/rts-runtime.js'),
    import('./state/game-state.js'),
  ]);
  validatePlayerPlan = validationModule.validatePlayerPlan;
  serverUnitTypes = rulesModule.UNIT_TYPES;
  serverBuildings = rulesModule.BUILDINGS;
  RtsRuntime = runtimeModule.RtsRuntime;
  createServerUnit = stateModule.createUnit;
  createServerStructure = stateModule.createStructure;

  loadMatches();
  let recoveredInterruptedMatch = false;
  for (const match of matches.values()) {
    if (match.pendingAdvance && match.completions.every(Boolean)) {
      finalizePhaseCheckpoint(match, { broadcastResult: false });
      recoveredInterruptedMatch = true;
    }
  }
  if (recoveredInterruptedMatch) persistMatches();

  const server = http.createServer(serveStatic);
  const websocketServer = new WebSocketServer({
    server,
    path: websocketPath,
    maxPayload: maxMessageBytes,
  });

  websocketServer.on('connection', (socket) => {
  socket.isAlive = true;
  socket.rateWindowStartedAt = Date.now();
  socket.rateWindowCount = 0;

  socket.on('pong', () => {
    socket.isAlive = true;
  });
  socket.on('message', (message) => {
    try {
      handleMessage(socket, message);
    } catch (error) {
      if (error instanceof ProtocolError) sendError(socket, error.code, error.message);
      else {
        console.error(`WebSocket message failed: ${error.stack || error.message}`);
        sendError(socket, 'SERVER_ERROR', 'The match server could not process that message.');
      }
    }
  });
  socket.on('close', () => handleDisconnect(socket));
  socket.on('error', (error) => {
    console.warn(`WebSocket connection error: ${error.message}`);
  });
  });

  const heartbeat = setInterval(() => {
    for (const socket of websocketServer.clients) {
      if (!socket.isAlive) {
        socket.terminate();
        continue;
      }
      socket.isAlive = false;
      socket.ping();
    }
  }, 30000);
  heartbeat.unref();

  const simulationLoop = setInterval(() => {
    for (const match of matches.values()) {
      if (
        match.state?.game?.phase !== 'realtime' ||
        !match.players[1] ||
        !connectedPlayers(match).every(Boolean) ||
        match.state.game.winner
      )
        continue;
      const runtime = ensureMatchRuntime(match);
      runtime.step();
      runtime.drainEvents();
      match.updatedAt = new Date().toISOString();
      if (match.state.game.simulationTick % 4 === 0) {
        match.revision += 1;
        broadcast(match, {
          type: 'simulation-snapshot',
          state: match.state,
          tick: match.state.game.simulationTick,
          revision: match.revision,
        });
      }
      if (match.state.game.simulationTick % 100 === 0) persistMatches();
    }
  }, 50);
  simulationLoop.unref();

  websocketServer.on('close', () => {
    clearInterval(heartbeat);
    clearInterval(simulationLoop);
  });

  server.listen(port, host, () => {
    const displayHost = host === '0.0.0.0' ? '127.0.0.1' : host;
    console.log(`Rule of War is ready at http://${displayHost}:${port}`);
    console.log(`Multiplayer relay is listening on ws://${displayHost}:${port}${websocketPath}`);
  });
}

startServer().catch((error) => {
  console.error(`Could not start Rule of War: ${error.stack || error.message}`);
  process.exitCode = 1;
});
