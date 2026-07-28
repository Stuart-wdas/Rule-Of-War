import {
  generateTerrain,
  hydrateTerrain,
  serializeTerrain,
  terrainColor,
} from "./terrain-generator.js";

function createTerrainRaster(terrain, maxTextureSide = 512) {
  const spanX = terrain.bounds.maxX - terrain.bounds.minX;
  const spanY = terrain.bounds.maxY - terrain.bounds.minY;
  const longestSide = Math.max(spanX, spanY, 1);
  const width = Math.max(
    256,
    Math.round((spanX / longestSide) * maxTextureSide),
  );
  const height = Math.max(
    256,
    Math.round((spanY / longestSide) * maxTextureSide),
  );
  const colorPixels = new Uint8ClampedArray(width * height * 4);
  const heightPixels = new Uint8ClampedArray(width * height * 4);

  for (let pixelY = 0; pixelY < height; pixelY += 1) {
    const mapY =
      terrain.bounds.minY +
      ((pixelY + 0.5) / height) * spanY;
    for (let pixelX = 0; pixelX < width; pixelX += 1) {
      const mapX =
        terrain.bounds.minX +
        ((pixelX + 0.5) / width) * spanX;
      const sample = terrain.sample(mapX, mapY);
      const color = terrainColor(sample, { x: mapX, y: mapY });
      const offset = (pixelY * width + pixelX) * 4;
      colorPixels[offset] = color[0];
      colorPixels[offset + 1] = color[1];
      colorPixels[offset + 2] = color[2];
      colorPixels[offset + 3] = 255;
      const elevation = Math.round(
        (sample.surfaceElevation ?? sample.elevation) * 255,
      );
      heightPixels[offset] = elevation;
      heightPixels[offset + 1] = elevation;
      heightPixels[offset + 2] = elevation;
      heightPixels[offset + 3] = 255;
    }
  }

  return { width, height, colorPixels, heightPixels };
}

self.onmessage = ({ data }) => {
  try {
    const rasterOnly = Boolean(data.serializedTerrain);
    const terrain = rasterOnly
      ? hydrateTerrain(data.serializedTerrain)
      : generateTerrain(data);
    const raster = createTerrainRaster(terrain, data.maxTextureSide);
    const response = {
      ok: true,
      raster,
      ...(rasterOnly ? {} : { terrain: serializeTerrain(terrain) }),
    };
    const transfers = [raster.colorPixels.buffer, raster.heightPixels.buffer];
    if (response.terrain)
      for (const value of Object.values(response.terrain))
        if (ArrayBuffer.isView(value)) transfers.push(value.buffer);
    self.postMessage(response, transfers);
  } catch (error) {
    self.postMessage({
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    });
  }
};
