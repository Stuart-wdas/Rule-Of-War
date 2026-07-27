import { createEntityIndex } from "./entity-index.js";

/**
 * Captures neutral command posts automatically when exactly one faction has a
 * ground unit nearby. Holding every command post for two rounds wins the
 * sector, providing an alternative to total destruction without extra input.
 */
export function resolveObjectives(
  state,
  unitTypes,
  { captureRadius = 2, roundsToWin = 2 } = {},
) {
  const index = createEntityIndex(state);
  const objectives = state.structures.filter(
    (structure) => structure.type === "objective" && structure.hp > 0,
  );
  const events = [];

  for (const objective of objectives) {
    const nearbyOwners = new Set(
      state.units
        .filter(
          (unit) =>
            unit.hp > 0 &&
            unitTypes[unit.type]?.airUnit !== true &&
            Math.hypot(unit.x - objective.x, unit.y - objective.y) <=
              captureRadius,
        )
        .map((unit) => unit.owner),
    );
    if (nearbyOwners.size !== 1) continue;
    const [owner] = nearbyOwners;
    if (objective.owner === owner) continue;
    const previousOwner = objective.owner;
    objective.owner = owner;
    events.push({
      type: "objective-captured",
      objectiveId: objective.id,
      previousOwner,
      owner,
      x: objective.x,
      y: objective.y,
    });
  }

  const controllingOwner =
    objectives.length > 0 &&
    objectives.every(
      (objective) =>
        objective.owner === objectives[0].owner && objective.owner >= 0,
    )
      ? objectives[0].owner
      : null;
  if (controllingOwner === null) {
    state.objectiveControl = { owner: null, rounds: 0 };
  } else if (state.objectiveControl?.owner === controllingOwner) {
    state.objectiveControl.rounds += 1;
  } else {
    state.objectiveControl = { owner: controllingOwner, rounds: 1 };
  }
  state.objectiveWinner =
    state.objectiveControl.rounds >= roundsToWin
      ? state.objectiveControl.owner
      : null;
  return { events, winner: state.objectiveWinner, index };
}
