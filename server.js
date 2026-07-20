const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { WebSocket, WebSocketServer } = require('ws');

const root = __dirname;
const host = process.env.HOST || '0.0.0.0';
const port = Number(process.env.PORT) || 4173;
const websocketPath = '/ws';
const matchStorePath = process.env.MATCH_STORE_PATH
  ? path.resolve(process.env.MATCH_STORE_PATH)
  : path.join(root, '.data', 'matches.json');
const dataDirectory = path.dirname(matchStorePath);
const maxMessageBytes = 1024 * 1024;
const matchCodeAlphabet = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';
const matches = new Map();
const liveSockets = new Map();

const mime = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
};

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
    fs.mkdirSync(dataDirectory, { recursive: true });
    const temporaryPath = `${matchStorePath}.${process.pid}.tmp`;
    const payload = JSON.stringify(
      {
        format: 1,
        savedAt: new Date().toISOString(),
        matches: [...matches.values()].map(serializableMatch),
      },
      null,
      2,
    );

    fs.writeFileSync(temporaryPath, payload, 'utf8');
    try {
      fs.renameSync(temporaryPath, matchStorePath);
    } catch (renameError) {
      fs.writeFileSync(matchStorePath, payload, 'utf8');
      fs.rmSync(temporaryPath, { force: true });
      if (renameError.code !== 'EPERM' && renameError.code !== 'EEXIST') {
        console.warn(`Match store used a non-atomic fallback: ${renameError.message}`);
      }
    }
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
    ready: [false, false],
    pendingAdvance,
    completions,
    lastCheckpoint: isRecord(raw.lastCheckpoint) ? cloneJson(raw.lastCheckpoint) : null,
  };

  if (!match.players[0]) return null;

  if (pendingAdvance && completions[0] && completions[1]) {
    const hashesMatch = completions[0].hash === completions[1].hash;
    match.state = completions[0].state;
    match.version += 1;
    match.revision += 1;
    match.pendingAdvance = null;
    match.completions = [null, null];
    match.lastCheckpoint = {
      sequence,
      hash: completions[0].hash,
      hashesMatch,
      version: match.version,
      revision: match.revision,
    };
  }

  return match;
}

function loadMatches() {
  if (!fs.existsSync(matchStorePath)) return;
  try {
    const stored = JSON.parse(fs.readFileSync(matchStorePath, 'utf8'));
    if (!Array.isArray(stored.matches)) return;
    for (const raw of stored.matches) {
      const match = normalizeLoadedMatch(raw);
      if (match) matches.set(match.code, match);
    }
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

function requireLivePeer(match) {
  if (!match.players[1]) {
    throw new ProtocolError('MATCH_WAITING', 'Waiting for the second player to join.');
  }
  if (!connectedPlayers(match).every(Boolean)) {
    throw new ProtocolError('PEER_OFFLINE', 'Both players must be connected.');
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
  requireLivePeer(match);
  if (match.pendingAdvance) {
    throw new ProtocolError('PHASE_LOCKED', 'This phase is currently resolving.');
  }
  if (match.ready[playerIndex]) {
    throw new ProtocolError('PLAYER_READY', 'Unready before changing your plan.');
  }
  if (!Number.isFinite(message.resources) || message.resources < 0 || message.resources > 1e9) {
    throw new ProtocolError('INVALID_PLAN', 'Resources must be a valid non-negative number.');
  }

  const jobs = sanitizeOwnedItems(message.jobs, playerIndex, 'Jobs');
  const orders = sanitizeOwnedItems(message.orders, playerIndex, 'Orders');
  const game = match.state.game;
  if (!isRecord(game.players[playerIndex])) {
    throw new ProtocolError('INVALID_STATE', 'The player record is missing from this match.');
  }

  game.jobs = replaceOwnedItems(game.jobs, jobs, playerIndex);
  game.orders = replaceOwnedItems(game.orders, orders, playerIndex);
  game.players[playerIndex] = {
    ...game.players[playerIndex],
    resources: message.resources,
  };
  match.revision += 1;
  match.updatedAt = new Date().toISOString();
  persistMatches();
  broadcast(match, {
    type: 'state-update',
    source: 'plan',
    playerIndex,
    state: match.state,
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
  requireLivePeer(match);
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

  if (match.ready.every(Boolean) && connectedPlayers(match).every(Boolean)) {
    startPhaseAdvance(match);
  }
}

function checkpointPayload(match, checkpoint = match.lastCheckpoint) {
  return {
    type: 'checkpoint',
    state: match.state,
    hash: checkpoint?.hash || hashSnapshot(match.state),
    hashesMatch: checkpoint?.hashesMatch ?? true,
    sequence: checkpoint?.sequence ?? match.sequence,
    version: match.version,
    revision: match.revision,
  };
}

function finalizePhaseCheckpoint(match, { broadcastResult = true } = {}) {
  if (!match.pendingAdvance || !match.completions.every(Boolean)) return false;
  const [hostCompletion, guestCompletion] = match.completions;
  const hashesMatch = hostCompletion.hash === guestCompletion.hash;
  match.state = hostCompletion.state;
  match.version += 1;
  match.revision += 1;
  match.pendingAdvance = null;
  match.completions = [null, null];
  match.ready = [false, false];
  match.updatedAt = new Date().toISOString();
  match.lastCheckpoint = {
    sequence: match.sequence,
    hash: hostCompletion.hash,
    hashesMatch,
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
  if (!match.pendingAdvance) match.ready[socket.playerIndex] = false;
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
  if (requested === '.data' || requested.startsWith('.data/')) {
    response.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' }).end('Not found');
    return;
  }

  const filePath = path.resolve(root, requested);
  const relative = path.relative(root, filePath);
  if (relative.startsWith('..') || path.isAbsolute(relative)) {
    response.writeHead(403).end('Forbidden');
    return;
  }

  fs.readFile(filePath, (error, data) => {
    if (error) {
      response.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' }).end('Not found');
      return;
    }
    response.writeHead(200, {
      'Content-Type': mime[path.extname(filePath)] || 'application/octet-stream',
      'Cache-Control': 'no-store',
    });
    if (request.method === 'HEAD') response.end();
    else response.end(data);
  });
}

loadMatches();
let recoveredInterruptedMatch = false;
for (const match of matches.values()) {
  if (match.pendingAdvance && match.completions.every(Boolean)) {
    finalizePhaseCheckpoint(match, { broadcastResult: false });
    recoveredInterruptedMatch = true;
  } else if (!match.pendingAdvance && match.ready.some(Boolean)) {
    match.ready = [false, false];
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

websocketServer.on('close', () => clearInterval(heartbeat));

server.listen(port, host, () => {
  const displayHost = host === '0.0.0.0' ? '127.0.0.1' : host;
  console.log(`Result of War is ready at http://${displayHost}:${port}`);
  console.log(`Multiplayer relay is listening on ws://${displayHost}:${port}${websocketPath}`);
});
