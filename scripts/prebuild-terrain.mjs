import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import { MAP_CATALOG } from "../maps/map-catalog.js";
import {
  generateTerrain,
  serializeTerrain,
} from "../terrain/terrain-generator.js";
import { encodeTerrainBinary } from "../terrain/terrain-binary.js";

const revision = 3;
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const outputDirectory = path.join(root, "assets", "terrain");
await mkdir(outputDirectory, { recursive: true });

for (const map of MAP_CATALOG) {
  if (!Number.isInteger(map.cols) || !Number.isInteger(map.rows)) continue;
  const bounds = {
    minX: -0.5,
    maxX: map.cols - 0.5,
    minY: -0.5,
    maxY: map.rows - 0.5,
  };
  process.stdout.write(`Prebuilding ${map.id} (${map.cols} x ${map.rows})... `);
  const terrain = generateTerrain({
    bounds,
    seed: map.id,
    config: map.terrainConfig ?? {},
  });
  const binary = encodeTerrainBinary(serializeTerrain(terrain));
  const outputPath = path.join(
    outputDirectory,
    `${map.id}-v${revision}.terrain`,
  );
  await writeFile(outputPath, binary);
  const compressed = gzipSync(binary, { level: 9 });
  await writeFile(`${outputPath}.gz`, compressed);
  console.log(
    `${(binary.byteLength / 1024 / 1024).toFixed(1)} MB raw / ${(compressed.byteLength / 1024 / 1024).toFixed(1)} MB compressed`,
  );
}
