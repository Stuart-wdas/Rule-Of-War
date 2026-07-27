/** Applies abstract HP damage while keeping a large formation headcount. */
export function applyFormationDamage(entity, amount, unitTypes) {
  const damage = Math.max(0, Number(amount) || 0);
  entity.hp -= damage;
  if (
    Number.isInteger(entity.strength) &&
    Number.isInteger(entity.maxStrength) &&
    entity.maxStrength > 1
  ) {
    const maximumHp = Math.max(1, unitTypes[entity.type]?.lives ?? entity.hp);
    const casualties = Math.max(
      1,
      Math.ceil((entity.maxStrength * damage) / maximumHp),
    );
    entity.strength = Math.max(0, entity.strength - casualties);
  }
  return entity;
}

/** Restores formation headcount in step with automatic medical healing. */
export function restoreFormationStrength(entity, healedHp, unitTypes) {
  if (
    !Number.isInteger(entity.strength) ||
    !Number.isInteger(entity.maxStrength) ||
    entity.maxStrength <= 1
  )
    return entity;
  const maximumHp = Math.max(1, unitTypes[entity.type]?.lives ?? entity.hp);
  const restored = Math.max(
    1,
    Math.ceil((entity.maxStrength * healedHp) / maximumHp),
  );
  entity.strength = Math.min(entity.maxStrength, entity.strength + restored);
  return entity;
}
