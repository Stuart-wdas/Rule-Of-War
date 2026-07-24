/**
 * Named battlefield presets shown after choosing a play mode.
 * Add a card by adding one entry here; gameplay settings remain untouched.
 */
function brokenFrontTiles() {
  const tiles = [];
  let logicalX = 0;
  for (let row = 0; row < 10; row += 1) {
    for (let column = 0; column < 18; column += 1) {
      const x = column;
      const y = row;
      const trianglePair =
        column < 17 &&
        ((row % 2 === 0 && column === 4) || (row % 2 === 1 && column === 12));
      if (trianglePair) {
        tiles.push({
          id: `triangle-${column}-${row}-a`,
          x: logicalX++,
          y: 0,
          terrainPoint: { x: column, y: row },
          vertices: [
            { x, y },
            { x: x + 2, y },
            { x: x + 2, y: y + 1 },
          ],
        });
        tiles.push({
          id: `triangle-${column}-${row}-b`,
          x: logicalX++,
          y: 0,
          terrainPoint: { x: column, y: row },
          vertices: [
            { x, y },
            { x: x + 2, y: y + 1 },
            { x, y: y + 1 },
          ],
        });
        column += 1;
      } else {
        tiles.push({
          id: `quadrilateral-${column}-${row}`,
          x: logicalX++,
          y: 0,
          terrainPoint: { x: column, y: row },
          vertices: [
            { x, y },
            { x: x + 1, y },
            { x: x + 1, y: y + 1 },
            { x, y: y + 1 },
          ],
        });
      }
    }
  }
  return tiles;
}

function hexFrontTiles({ columns, rows, radius }) {
  const tiles = [];
  const rowSpacing = Math.sqrt(3) * radius;
  for (let column = 0; column < columns; column += 1) {
    for (let row = 0; row < rows; row += 1) {
      const center = {
        x: column * radius * 1.5,
        y: row * rowSpacing + (column % 2) * (rowSpacing / 2),
      };
      const vertices = Array.from({ length: 6 }, (_, index) => {
        const angle = (Math.PI / 3) * index;
        return {
          x: center.x + Math.cos(angle) * radius,
          y: center.y + Math.sin(angle) * radius,
        };
      });
      tiles.push({
        id: `hex-${column}-${row}`,
        x: column,
        y: row,
        center,
        terrainPoint: center,
        vertices,
      });
    }
  }
  return tiles;
}

function createHexMapPreset({ cols, rows, tileRadius = 1, ...metadata }) {
  return Object.freeze({
    ...metadata,
    cols,
    rows,
    tileRadius,
    layout: "hex-odd-q",
    tileCount: cols * rows,
    // Large maps are generated only after selection. Keeping factories in the
    // catalog prevents every map's polygon data from occupying memory at boot.
    createTiles: () =>
      hexFrontTiles({ columns: cols, rows, radius: tileRadius }),
  });
}

export const MAP_CATALOG = Object.freeze([
  // Object.freeze({
  //   id: "close-quarters",
  //   name: "CLOSE QUARTERS",
  //   theater: "DENSE CONTACT",
  //   cols: 18,
  //   rows: 18,
  //   description: "A compact sector for fast contact and short supply lines.",
  //   preview: "compact",
  // }),
  // Object.freeze({
  //   id: "kestrel-pass",
  //   name: "KESTREL PASS",
  //   theater: "BALANCED FRONT",
  //   cols: 28,
  //   rows: 22,
  //   description: "A balanced field with room for flanking and artillery.",
  //   preview: "pass",
  // }),
  // Object.freeze({
  //   id: "long-march",
  //   name: "THE LONG MARCH",
  //   theater: "DEEP OPERATION",
  //   cols: 128,
  //   rows: 64,
  //   description: "A wide advance where mobility and forward bases matter.",
  //   preview: "wide",
  // }),
  Object.freeze({
    id: "galaxy-front",
    name: "GALAXY FRONT",
    theater: "GALAXY CAMPAIGN",
    cols: 1024,
    rows: 1024,
    description: "The full strategic battlefield for galaxy-form campaigns.",
    preview: "grand",
  }),
  Object.freeze({
    id: "grand-front",
    name: "GRAND FRONT",
    theater: "FULL CAMPAIGN",
    cols: 128,
    rows: 128,
    description: "The full strategic battlefield for long-form campaigns.",
    preview: "grand",
  }),
  Object.freeze({
    id: "world-war",
    name: "World War",
    theater: "FULL CAMPAIGN",
    cols: 512,
    rows: 512,
    description:
      "A 262,144-tile strategic theater rendered as a moving tactical window.",
    preview: "world",
  }),
  // Object.freeze({
  //   id: "broken-front",
  //   name: "BROKEN FRONT",
  //   theater: "CUSTOM TOPOLOGY",
  //   description:
  //     "A broad front blending equal-area square and triangular command tiles.",
  //   preview: "topology",
  //   cols: 18,
  //   rows: 10,
  //   tileCount: 180,
  //   createTiles: brokenFrontTiles,
  // }),
  // createHexMapPreset({
  //   id: "honeycomb-front",
  //   name: "HONEYCOMB FRONT",
  //   theater: "HEX OPERATION",
  //   description:
  //     "A complete regular-hex battlefield with six-direction navigation.",
  //   preview: "hex",
  //   cols: 52,
  //   rows: 54,
  //   tileRadius: 1,
  // }),
  // createHexMapPreset({
  //   id: "world-war-hex",
  //   name: "World War Hex",
  //   theater: "FULL CAMPAIGN HEX",
  //   cols: 128,
  //   rows: 128,
  //   preview: "hex",
  //   description: "A strategic theater rendered as a moving tactical window.",
  //   preview: "hex",
  //   tileRadius: 1,
  // }),
  // createHexMapPreset({
  //   id: "long-march-hex",
  //   name: "THE LONG MARCH HEX",
  //   theater: "DEEP OPERATION HEX",
  //   cols: 128,
  //   rows: 64,
  //   preview: "hex",
  //   // Render streaming only: change this without changing gameplay topology.
  //   // Larger chunks rebuild less often; smaller chunks upload less per pan.
  //   renderChunkSize: 128,
  //   description:
  //     "A 262,144-tile strategic theater rendered as a moving tactical window.",
  //   preview: "world",
  //   tileRadius: 1,
  // }),
]);

export const DEFAULT_MAP_ID = "grand-front";
const customMapCatalog = [];

export function registerCustomMaps(maps = []) {
  customMapCatalog.splice(0, customMapCatalog.length);
  for (const map of maps)
    if (
      map?.id?.startsWith("custom-") &&
      Number.isInteger(map.cols) &&
      Number.isInteger(map.rows)
    )
      customMapCatalog.push(map);
}

export function allMapPresets() {
  return [...MAP_CATALOG, ...customMapCatalog];
}

export function getMapPreset(id) {
  return (
    allMapPresets().find((map) => map.id === id) ??
    MAP_CATALOG.find((map) => map.id === DEFAULT_MAP_ID)
  );
}

export function findMapPresetByDimensions(cols, rows) {
  return (
    allMapPresets().find((map) => map.cols === cols && map.rows === rows) ??
    getMapPreset(DEFAULT_MAP_ID)
  );
}

/** Returns a new config so saved developer preferences are never mutated. */
export function applyMapPreset(config, id) {
  const map = getMapPreset(id);
  return { ...config, cols: map.cols, rows: map.rows };
}
