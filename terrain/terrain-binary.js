const MAGIC = "ROWTERR1";
const ARRAY_FIELDS = Object.freeze([
  ["elevation", Float32Array],
  ["surfaceElevation", Float32Array],
  ["moisture", Float32Array],
  ["river", Uint8Array],
  ["riverFlow", Float32Array],
  ["downstream", Int32Array],
  ["terrainTypeCodes", Uint8Array],
]);

export function encodeTerrainBinary(terrain) {
  const sections = [];
  let dataLength = 0;
  for (const [name] of ARRAY_FIELDS) {
    const value = terrain[name];
    if (!ArrayBuffer.isView(value))
      throw new Error(`Terrain binary is missing ${name}.`);
    sections.push({ name, byteLength: value.byteLength });
    dataLength += value.byteLength;
  }
  const headerBytes = new TextEncoder().encode(
    JSON.stringify({
      version: terrain.version,
      seed: terrain.seed,
      bounds: terrain.bounds,
      width: terrain.width,
      height: terrain.height,
      config: terrain.config,
      sections,
    }),
  );
  const output = new Uint8Array(12 + headerBytes.length + dataLength);
  output.set(new TextEncoder().encode(MAGIC), 0);
  new DataView(output.buffer).setUint32(8, headerBytes.length, true);
  output.set(headerBytes, 12);
  let offset = 12 + headerBytes.length;
  for (const [name] of ARRAY_FIELDS) {
    const value = terrain[name];
    output.set(
      new Uint8Array(value.buffer, value.byteOffset, value.byteLength),
      offset,
    );
    offset += value.byteLength;
  }
  return output;
}

export function decodeTerrainBinary(buffer) {
  const bytes = buffer instanceof Uint8Array ? buffer : new Uint8Array(buffer);
  const magic = new TextDecoder().decode(bytes.subarray(0, 8));
  if (magic !== MAGIC) throw new Error("Unsupported terrain binary.");
  const headerLength = new DataView(
    bytes.buffer,
    bytes.byteOffset,
    bytes.byteLength,
  ).getUint32(8, true);
  const header = JSON.parse(
    new TextDecoder().decode(bytes.subarray(12, 12 + headerLength)),
  );
  const result = {
    version: header.version,
    seed: header.seed,
    bounds: header.bounds,
    width: header.width,
    height: header.height,
    config: header.config,
  };
  let offset = 12 + headerLength;
  for (const [name, ArrayType] of ARRAY_FIELDS) {
    const section = header.sections.find((item) => item.name === name);
    if (!section) throw new Error(`Terrain binary is missing ${name}.`);
    const copiedBuffer = bytes.buffer.slice(
      bytes.byteOffset + offset,
      bytes.byteOffset + offset + section.byteLength,
    );
    result[name] = new ArrayType(copiedBuffer);
    offset += section.byteLength;
  }
  return result;
}

export async function loadPrebuiltTerrain(mapId, revision = 3) {
  const response = await fetch(
    `./assets/terrain/${encodeURIComponent(mapId)}-v${revision}.terrain`,
    { cache: "force-cache" },
  );
  if (!response.ok) return null;
  return decodeTerrainBinary(await response.arrayBuffer());
}
