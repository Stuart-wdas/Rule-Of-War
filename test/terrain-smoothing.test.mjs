import assert from "node:assert/strict";
import test from "node:test";
import { smoothTerrainBrush } from "../core/terrain-smoothing.js";

test("terrain smoothing softens peaks symmetrically", () => {
  const size = 5;
  const elevation = new Float32Array(size * size);
  elevation[2 * size + 2] = 1;

  smoothTerrainBrush(elevation, size, 2, 2, 2, 1);

  assert.ok(elevation[2 * size + 2] < 1);
  assert.ok(elevation[2 * size + 2] > 0);
  assert.equal(elevation[2 * size + 1], elevation[2 * size + 3]);
  assert.equal(elevation[1 * size + 2], elevation[3 * size + 2]);
  assert.equal(elevation[0], 0);
});

test("terrain smoothing remains bounded at map edges", () => {
  const size = 3;
  const elevation = new Float32Array([
    1, 0, 0,
    0, 0, 0,
    0, 0, 0,
  ]);

  smoothTerrainBrush(elevation, size, 0, 0, 2, 0.75);

  for (const height of elevation) {
    assert.ok(Number.isFinite(height));
    assert.ok(height >= 0 && height <= 1);
  }
  assert.ok(elevation[0] < 1);
  assert.ok(elevation[1] > 0);
});
