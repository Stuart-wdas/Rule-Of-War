import {
  CONFIG_LIMITS,
  DEFAULT_GAME_CONFIG,
} from "../game-rules.js";

function factionAssetCount(config, prefix) {
  // Every faction always receives one commander.
  return (
    1 +
    config[`${prefix}Mines`] * 9 +
    config[`${prefix}Barracks`] * 9 +
    config[`${prefix}Short`] +
    config[`${prefix}Medium`] +
    config[`${prefix}Long`] +
    config[`${prefix}Xtralong`]
  );
}

/** Returns a user-facing error string, or null when configuration is valid. */
export function validateDeveloperConfig(config) {
  for (const [key, [minimum, maximum]] of Object.entries(CONFIG_LIMITS)) {
    if (
      !Number.isInteger(config[key]) ||
      config[key] < minimum ||
      config[key] > maximum
    ) {
      const label = key.replace(/([A-Z])/g, " $1").toLowerCase();
      return `${label} must be a whole number from ${minimum} to ${maximum}.`;
    }
  }
  if (typeof config.fogEnabled !== "boolean")
    return "Fog of war must be enabled or disabled.";
  if (
    config.terrainDebug !== undefined &&
    typeof config.terrainDebug !== "boolean"
  )
    return "Terrain debug colours must be enabled or disabled.";
  if (
    config.terrainDebugElevation !== undefined &&
    typeof config.terrainDebugElevation !== "boolean"
  )
    return "Terrain elevation labels must be enabled or disabled.";
  const factionCapacity = Math.floor(config.cols / 2) * config.rows;
  const playerCount = factionAssetCount(config, "player");
  const enemyCount = factionAssetCount(config, "enemy");
  if (playerCount > factionCapacity)
    return `Field Command has ${playerCount} assets, but its deployment zone only holds ${factionCapacity}. Enlarge the map or reduce the force.`;
  if (enemyCount > factionCapacity)
    return `Iron Front has ${enemyCount} assets, but its deployment zone only holds ${factionCapacity}. Enlarge the map or reduce the force.`;
  return null;
}

/** Browser persistence adapter for developer settings. */
export class DeveloperConfigStore {
  constructor(storageKey, storage = window.localStorage) {
    this.storageKey = storageKey;
    this.storage = storage;
  }

  load() {
    try {
      const saved = this.storage.getItem(this.storageKey);
      if (!saved) return { ...DEFAULT_GAME_CONFIG };
      const parsed = JSON.parse(saved);
      const config = { ...DEFAULT_GAME_CONFIG };
      for (const key of Object.keys(DEFAULT_GAME_CONFIG)) {
        const savedValue = Object.prototype.hasOwnProperty.call(parsed, key)
          ? parsed[key]
          : DEFAULT_GAME_CONFIG[key];
        config[key] =
          key === "fogEnabled" ||
          key === "terrainDebug" ||
          key === "terrainDebugElevation"
            ? Boolean(savedValue)
            : Number(savedValue);
      }
      return validateDeveloperConfig(config)
        ? { ...DEFAULT_GAME_CONFIG }
        : config;
    } catch {
      return { ...DEFAULT_GAME_CONFIG };
    }
  }

  save(config) {
    try {
      this.storage.setItem(this.storageKey, JSON.stringify(config));
    } catch {
      // Private browser sessions may disallow storage; active state still works.
    }
  }
}
