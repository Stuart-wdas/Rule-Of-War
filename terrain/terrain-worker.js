import { generateTerrain, serializeTerrain } from "./terrain-generator.js";

self.onmessage = ({ data }) => {
  try {
    const terrain = generateTerrain(data);
    self.postMessage({ ok: true, terrain: serializeTerrain(terrain) });
  } catch (error) {
    self.postMessage({
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    });
  }
};
