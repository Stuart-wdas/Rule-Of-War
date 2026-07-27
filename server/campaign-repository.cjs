const fs = require("node:fs");
const path = require("node:path");
const { DatabaseSync } = require("node:sqlite");

/**
 * Small SQLite repository for campaigns, sectors, and battle snapshots.
 * Payload columns retain schema-versioned JSON while indexed columns provide
 * cheap lookup and allow the data model to expand without whole-file rewrites.
 */
function createCampaignRepository(databasePath) {
  fs.mkdirSync(path.dirname(databasePath), { recursive: true });
  const database = new DatabaseSync(databasePath);
  database.exec(`
    PRAGMA journal_mode = WAL;
    PRAGMA synchronous = NORMAL;
    PRAGMA foreign_keys = ON;

    CREATE TABLE IF NOT EXISTS matches (
      code TEXT PRIMARY KEY,
      version INTEGER NOT NULL,
      sequence INTEGER NOT NULL,
      updated_at TEXT NOT NULL,
      payload TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS worlds (
      id TEXT PRIMARY KEY,
      turn INTEGER NOT NULL,
      updated_at TEXT NOT NULL,
      payload TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS sectors (
      world_id TEXT NOT NULL,
      sector_id TEXT NOT NULL,
      owner INTEGER,
      contested INTEGER NOT NULL DEFAULT 0,
      battle_id TEXT,
      payload TEXT NOT NULL,
      PRIMARY KEY (world_id, sector_id),
      FOREIGN KEY (world_id) REFERENCES worlds(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS campaign_events (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      world_id TEXT NOT NULL,
      turn INTEGER NOT NULL,
      event_type TEXT NOT NULL,
      payload TEXT NOT NULL,
      created_at TEXT NOT NULL,
      FOREIGN KEY (world_id) REFERENCES worlds(id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS sectors_by_owner
      ON sectors(world_id, owner);
    CREATE INDEX IF NOT EXISTS campaign_events_by_turn
      ON campaign_events(world_id, turn, id);
  `);

  const upsertMatch = database.prepare(`
    INSERT INTO matches(code, version, sequence, updated_at, payload)
    VALUES (?, ?, ?, ?, ?)
    ON CONFLICT(code) DO UPDATE SET
      version = excluded.version,
      sequence = excluded.sequence,
      updated_at = excluded.updated_at,
      payload = excluded.payload
  `);
  const deleteMatch = database.prepare("DELETE FROM matches WHERE code = ?");
  const selectMatches = database.prepare(
    "SELECT payload FROM matches ORDER BY updated_at DESC",
  );
  const selectMatchCodes = database.prepare("SELECT code FROM matches");

  return {
    loadMatches() {
      return selectMatches.all().flatMap((row) => {
        try {
          return [JSON.parse(row.payload)];
        } catch {
          return [];
        }
      });
    },

    replaceMatches(matches) {
      const incomingCodes = new Set(matches.map((match) => match.code));
      database.exec("BEGIN IMMEDIATE");
      try {
        for (const match of matches)
          upsertMatch.run(
            match.code,
            match.version ?? 1,
            match.sequence ?? 0,
            match.updatedAt ?? new Date().toISOString(),
            JSON.stringify(match),
          );
        for (const row of selectMatchCodes.all())
          if (!incomingCodes.has(row.code)) deleteMatch.run(row.code);
        database.exec("COMMIT");
      } catch (error) {
        database.exec("ROLLBACK");
        throw error;
      }
    },

    saveWorld(world) {
      const updatedAt = new Date().toISOString();
      database.exec("BEGIN IMMEDIATE");
      try {
        database
          .prepare(`
            INSERT INTO worlds(id, turn, updated_at, payload)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
              turn = excluded.turn,
              updated_at = excluded.updated_at,
              payload = excluded.payload
          `)
          .run(world.id, world.turn, updatedAt, JSON.stringify(world));
        const upsertSector = database.prepare(`
          INSERT INTO sectors(world_id, sector_id, owner, contested, battle_id, payload)
          VALUES (?, ?, ?, ?, ?, ?)
          ON CONFLICT(world_id, sector_id) DO UPDATE SET
            owner = excluded.owner,
            contested = excluded.contested,
            battle_id = excluded.battle_id,
            payload = excluded.payload
        `);
        for (const sector of world.sectors ?? [])
          upsertSector.run(
            world.id,
            sector.id,
            sector.owner,
            sector.contested ? 1 : 0,
            sector.battleId ?? null,
            JSON.stringify(sector),
          );
        database.exec("COMMIT");
      } catch (error) {
        database.exec("ROLLBACK");
        throw error;
      }
    },

    loadWorld(id) {
      const row = database
        .prepare("SELECT payload FROM worlds WHERE id = ?")
        .get(id);
      if (!row) return null;
      try {
        return JSON.parse(row.payload);
      } catch {
        return null;
      }
    },

    close() {
      database.close();
    },
  };
}

module.exports = { createCampaignRepository };
