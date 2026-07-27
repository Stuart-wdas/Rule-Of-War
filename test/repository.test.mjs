import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { createCampaignRepository } = require(
  "../server/campaign-repository.cjs",
);

test("SQLite repository persists matches and indexed world sectors", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "rule-of-war-"));
  const databasePath = path.join(directory, "campaign.sqlite");
  const repository = createCampaignRepository(databasePath);
  try {
    repository.replaceMatches([
      {
        code: "ABC123",
        version: 2,
        sequence: 4,
        updatedAt: "2026-01-01T00:00:00.000Z",
        state: { schema: 1 },
      },
    ]);
    assert.equal(repository.loadMatches()[0].code, "ABC123");

    const world = {
      id: "world-test",
      turn: 3,
      sectors: [
        {
          id: "sector-0-0",
          owner: 0,
          contested: false,
          battleId: null,
        },
      ],
    };
    repository.saveWorld(world);
    assert.deepEqual(repository.loadWorld("world-test"), world);
  } finally {
    repository.close();
    fs.rmSync(directory, { recursive: true, force: true });
  }
});
