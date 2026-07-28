import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import WebSocket from "ws";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function availablePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

function nextMessage(socket, type, timeout = 5000) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`Timed out waiting for ${type}`)),
      timeout,
    );
    const onMessage = (data) => {
      const message = JSON.parse(data.toString());
      if (message.type !== type) return;
      clearTimeout(timer);
      socket.off("message", onMessage);
      resolve(message);
    };
    socket.on("message", onMessage);
  });
}

function openSocket(url) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    socket.once("open", () => resolve(socket));
    socket.once("error", reject);
  });
}

test("server protects private files and broadcasts validated plan deltas", async () => {
  const port = await availablePort();
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "rule-war-server-"));
  const child = spawn(process.execPath, ["server.cjs"], {
    cwd: root,
    env: {
      ...process.env,
      HOST: "127.0.0.1",
      PORT: String(port),
      DATABASE_PATH: path.join(directory, "campaign.sqlite"),
      MATCH_STORE_PATH: path.join(directory, "legacy.json"),
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const output = [];
  child.stdout.on("data", (data) => output.push(data.toString()));
  child.stderr.on("data", (data) => output.push(data.toString()));
  const host = `http://127.0.0.1:${port}`;
  let hostSocket;
  let guestSocket;
  try {
    const deadline = Date.now() + 5000;
    while (Date.now() < deadline) {
      try {
        const response = await fetch(host);
        if (response.ok) break;
      } catch {
        await new Promise((resolve) => setTimeout(resolve, 25));
      }
    }
    assert.equal((await fetch(host)).status, 200, output.join(""));
    const globeModule = await fetch(`${host}/world-globe.js`);
    assert.equal(globeModule.status, 200);
    assert.match(await globeModule.text(), /export class WorldGlobe/);
    assert.equal((await fetch(`${host}/.git/HEAD`)).status, 404);
    assert.equal((await fetch(`${host}/server.cjs`)).status, 404);

    hostSocket = await openSocket(`ws://127.0.0.1:${port}/ws`);
    const hostSessionPromise = nextMessage(hostSocket, "session");
    hostSocket.send(
      JSON.stringify({
        type: "create-match",
        state: {
          schema: 1,
          config: { cols: 10, rows: 10 },
          game: {
            round: 1,
            phase: "deployment",
            players: [
              { id: 0, resources: 10 },
              { id: 1, resources: 10 },
            ],
            units: [],
            structures: [],
            jobs: [],
            orders: [],
          },
        },
      }),
    );
    const hostSession = await hostSessionPromise;

    guestSocket = await openSocket(`ws://127.0.0.1:${port}/ws`);
    const guestSessionPromise = nextMessage(guestSocket, "session");
    guestSocket.send(
      JSON.stringify({ type: "join-match", code: hostSession.code }),
    );
    await guestSessionPromise;

    const deltaPromise = nextMessage(hostSocket, "state-update");
    hostSocket.send(
      JSON.stringify({
        type: "plan-update",
        jobs: [],
        orders: [],
        resources: 999999,
      }),
    );
    const delta = await deltaPromise;
    assert.equal(delta.resources, 10);
    assert.deepEqual(delta.jobs, []);
    assert.deepEqual(delta.orders, []);
    assert.equal("state" in delta, false);

    const hostAdvancePromise = nextMessage(hostSocket, "phase-advance");
    const guestAdvancePromise = nextMessage(guestSocket, "phase-advance");
    hostSocket.send(JSON.stringify({ type: "set-ready", ready: true }));
    guestSocket.send(JSON.stringify({ type: "set-ready", ready: true }));
    const [hostAdvance, guestAdvance] = await Promise.all([
      hostAdvancePromise,
      guestAdvancePromise,
    ]);
    const hostResult = structuredClone(hostAdvance.state);
    const guestResult = structuredClone(guestAdvance.state);
    hostResult.game.phase = "orders";
    guestResult.game.phase = "attack";
    const checkpointPromise = nextMessage(hostSocket, "checkpoint");
    hostSocket.send(
      JSON.stringify({
        type: "phase-complete",
        sequence: hostAdvance.sequence,
        state: hostResult,
      }),
    );
    guestSocket.send(
      JSON.stringify({
        type: "phase-complete",
        sequence: guestAdvance.sequence,
        state: guestResult,
      }),
    );
    const checkpoint = await checkpointPromise;
    assert.equal(checkpoint.rejected, true);
    assert.equal(checkpoint.state.game.phase, "deployment");
  } finally {
    hostSocket?.close();
    guestSocket?.close();
    child.kill();
    await new Promise((resolve) => child.once("exit", resolve));
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

test("server authoritatively advances real-time movement commands", async () => {
  const port = await availablePort();
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "rule-war-rts-"));
  const child = spawn(process.execPath, ["server.cjs"], {
    cwd: root,
    env: {
      ...process.env,
      HOST: "127.0.0.1",
      PORT: String(port),
      DATABASE_PATH: path.join(directory, "campaign.sqlite"),
      MATCH_STORE_PATH: path.join(directory, "legacy.json"),
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const output = [];
  child.stdout.on("data", (data) => output.push(data.toString()));
  child.stderr.on("data", (data) => output.push(data.toString()));
  const host = `http://127.0.0.1:${port}`;
  let hostSocket;
  let guestSocket;
  try {
    const deadline = Date.now() + 5000;
    while (Date.now() < deadline) {
      try {
        if ((await fetch(host)).ok) break;
      } catch {
        await new Promise((resolve) => setTimeout(resolve, 25));
      }
    }
    hostSocket = await openSocket(`ws://127.0.0.1:${port}/ws`);
    const hostSessionPromise = nextMessage(hostSocket, "session");
    hostSocket.send(
      JSON.stringify({
        type: "create-match",
        state: {
          schema: 1,
          config: { cols: 20, rows: 20 },
          game: {
            round: 1,
            phase: "realtime",
            mode: "realtime",
            simulationTick: 0,
            tickRate: 20,
            players: [
              { id: 0, resources: 10 },
              { id: 1, resources: 10 },
            ],
            units: [
              {
                id: "host-unit",
                type: "commander",
                owner: 0,
                x: 1,
                y: 1,
                hp: 1,
                revealsFog: true,
              },
              {
                id: "guest-unit",
                type: "commander",
                owner: 1,
                x: 18,
                y: 18,
                hp: 1,
                revealsFog: true,
              },
            ],
            structures: [],
            jobs: [],
            orders: [],
            projectiles: [],
          },
        },
      }),
    );
    const hostSession = await hostSessionPromise;
    guestSocket = await openSocket(`ws://127.0.0.1:${port}/ws`);
    const guestSessionPromise = nextMessage(guestSocket, "session");
    guestSocket.send(
      JSON.stringify({ type: "join-match", code: hostSession.code }),
    );
    await guestSessionPromise;

    const acceptedPromise = nextMessage(hostSocket, "command-accepted");
    hostSocket.send(
      JSON.stringify({
        type: "rts-command",
        sequence: 1,
        commandType: "move",
        unitIds: ["host-unit"],
        destination: { x: 5, y: 1 },
      }),
    );
    const accepted = await acceptedPromise;
    assert.deepEqual(accepted.unitIds, ["host-unit"]);

    let movedUnit = null;
    const snapshotDeadline = Date.now() + 3000;
    while (Date.now() < snapshotDeadline && !movedUnit) {
      const snapshot = await nextMessage(hostSocket, "simulation-snapshot");
      const unit = snapshot.state.game.units.find(
        (item) => item.id === "host-unit",
      );
      if (unit?.worldX > 1.05) movedUnit = unit;
    }
    assert.ok(movedUnit, output.join(""));
    assert.ok(movedUnit.worldX <= 5);
  } finally {
    hostSocket?.close();
    guestSocket?.close();
    child.kill();
    await new Promise((resolve) => child.once("exit", resolve));
    fs.rmSync(directory, { recursive: true, force: true });
  }
});
