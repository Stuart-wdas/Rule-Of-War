const GAUSSIAN_KERNEL = [
  [1, 2, 1],
  [2, 4, 2],
  [1, 2, 1],
];

function clamp(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

export function smoothTerrainBrush(
  elevation,
  size,
  centerX,
  centerY,
  radius,
  strength,
) {
  const effectiveRadius = Math.max(1, radius);
  const minX = Math.max(0, Math.floor(centerX - effectiveRadius));
  const maxX = Math.min(size - 1, Math.ceil(centerX + effectiveRadius));
  const minY = Math.max(0, Math.floor(centerY - effectiveRadius));
  const maxY = Math.min(size - 1, Math.ceil(centerY + effectiveRadius));
  // Scale the sample spacing with large-map brushes so smoothing remains
  // visible when the editor preview is downsampled.
  const sampleStep = Math.max(1, Math.round(effectiveRadius / 12));
  const sourceMinX = Math.max(0, minX - sampleStep);
  const sourceMaxX = Math.min(size - 1, maxX + sampleStep);
  const sourceMinY = Math.max(0, minY - sampleStep);
  const sourceMaxY = Math.min(size - 1, maxY + sampleStep);
  const sourceWidth = sourceMaxX - sourceMinX + 1;
  const source = new Float32Array(
    sourceWidth * (sourceMaxY - sourceMinY + 1),
  );

  for (let y = sourceMinY; y <= sourceMaxY; y += 1)
    for (let x = sourceMinX; x <= sourceMaxX; x += 1)
      source[(y - sourceMinY) * sourceWidth + x - sourceMinX] =
        elevation[y * size + x];

  const sourceAt = (x, y) => {
    const sampleX = clamp(x, sourceMinX, sourceMaxX);
    const sampleY = clamp(y, sourceMinY, sourceMaxY);
    return source[
      (sampleY - sourceMinY) * sourceWidth + sampleX - sourceMinX
    ];
  };
  const smoothing = clamp(strength, 0.01, 1);

  for (let y = minY; y <= maxY; y += 1)
    for (let x = minX; x <= maxX; x += 1) {
      const distance = Math.hypot(x - centerX, y - centerY);
      if (distance > effectiveRadius) continue;
      let weightedHeight = 0;
      let totalWeight = 0;
      for (let offsetY = -1; offsetY <= 1; offsetY += 1)
        for (let offsetX = -1; offsetX <= 1; offsetX += 1) {
          const weight = GAUSSIAN_KERNEL[offsetY + 1][offsetX + 1];
          weightedHeight +=
            sourceAt(
              x + offsetX * sampleStep,
              y + offsetY * sampleStep,
            ) * weight;
          totalWeight += weight;
        }
      const normalizedDistance = distance / effectiveRadius;
      const falloff = (1 - normalizedDistance * normalizedDistance) ** 2;
      const index = y * size + x;
      const current = sourceAt(x, y);
      elevation[index] = clamp(
        current + (weightedHeight / totalWeight - current) * smoothing * falloff,
        0,
        1,
      );
    }

  return { minX, minY, maxX, maxY };
}
