/** Converts a zero-based column index to spreadsheet-style labels: A..Z, AA. */
export function columnLabel(index) {
  let value = index + 1;
  let label = "";
  while (value > 0) {
    value -= 1;
    label = String.fromCharCode(65 + (value % 26)) + label;
    value = Math.floor(value / 26);
  }
  return label;
}

/** Euclidean distance is the shared basis for every circular game range. */
export function distance(a, b) {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

/** Returns deployment cells ordered from the faction edge and map center. */
export function deploymentCells(cols, rows, owner) {
  const middle = (rows - 1) / 2;
  const yOrder = Array.from({ length: rows }, (_, y) => y).sort(
    (a, b) => Math.abs(a - middle) - Math.abs(b - middle) || a - b,
  );
  const zoneWidth = Math.floor(cols / 2);
  const xOrder = Array.from({ length: zoneWidth }, (_, x) =>
    owner === 0 ? x : cols - 1 - x,
  );
  return xOrder.flatMap((x) => yOrder.map((y) => ({ x, y })));
}
