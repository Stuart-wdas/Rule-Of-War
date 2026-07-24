const LEGACY_STORAGE_KEY = "result-of-war:custom-maps:v1";
const DATABASE_NAME = "result-of-war:map-builder";
const DATABASE_VERSION = 1;
const STORE_NAME = "custom-maps";
const MAX_SAVED_MAPS = 12;

function requestResult(request) {
  return new Promise((resolve, reject) => {
    request.addEventListener("success", () => resolve(request.result), {
      once: true,
    });
    request.addEventListener("error", () => reject(request.error), {
      once: true,
    });
  });
}

function transactionComplete(transaction) {
  return new Promise((resolve, reject) => {
    transaction.addEventListener("complete", resolve, { once: true });
    transaction.addEventListener(
      "abort",
      () => reject(transaction.error),
      { once: true },
    );
    transaction.addEventListener(
      "error",
      () => reject(transaction.error),
      { once: true },
    );
  });
}

function openDatabase() {
  if (!globalThis.indexedDB)
    return Promise.reject(new Error("IndexedDB is unavailable."));
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DATABASE_NAME, DATABASE_VERSION);
    request.addEventListener("upgradeneeded", () => {
      const database = request.result;
      if (!database.objectStoreNames.contains(STORE_NAME))
        database.createObjectStore(STORE_NAME, { keyPath: "id" });
    });
    request.addEventListener("success", () => resolve(request.result), {
      once: true,
    });
    request.addEventListener("error", () => reject(request.error), {
      once: true,
    });
  });
}

function loadLegacyMaps() {
  try {
    const maps = JSON.parse(localStorage.getItem(LEGACY_STORAGE_KEY));
    return Array.isArray(maps) ? maps : [];
  } catch {
    return [];
  }
}

async function readDatabaseMaps(database) {
  const transaction = database.transaction(STORE_NAME, "readonly");
  const maps = await requestResult(transaction.objectStore(STORE_NAME).getAll());
  await transactionComplete(transaction);
  return maps;
}

async function migrateLegacyMaps(database, legacyMaps) {
  if (!legacyMaps.length) return;
  const transaction = database.transaction(STORE_NAME, "readwrite");
  const store = transaction.objectStore(STORE_NAME);
  legacyMaps.forEach((map, index) =>
    store.put({
      ...map,
      savedAt: map.savedAt ?? Date.now() - index,
    }),
  );
  await transactionComplete(transaction);
  try {
    localStorage.removeItem(LEGACY_STORAGE_KEY);
  } catch {
    // Successful IndexedDB migration is enough when localStorage is locked.
  }
}

export async function loadCustomMaps() {
  const legacyMaps = loadLegacyMaps();
  try {
    const database = await openDatabase();
    await migrateLegacyMaps(database, legacyMaps);
    const maps = await readDatabaseMaps(database);
    database.close();
    return maps
      .sort((left, right) => (right.savedAt ?? 0) - (left.savedAt ?? 0))
      .slice(0, MAX_SAVED_MAPS);
  } catch {
    return legacyMaps;
  }
}

export async function saveCustomMap(map) {
  const database = await openDatabase();
  const savedMap = { ...map, savedAt: Date.now() };
  const writeTransaction = database.transaction(STORE_NAME, "readwrite");
  writeTransaction.objectStore(STORE_NAME).put(savedMap);
  await transactionComplete(writeTransaction);

  const maps = await readDatabaseMaps(database);
  maps.sort((left, right) => (right.savedAt ?? 0) - (left.savedAt ?? 0));
  if (maps.length > MAX_SAVED_MAPS) {
    const trimTransaction = database.transaction(STORE_NAME, "readwrite");
    const store = trimTransaction.objectStore(STORE_NAME);
    for (const oldMap of maps.slice(MAX_SAVED_MAPS)) store.delete(oldMap.id);
    await transactionComplete(trimTransaction);
  }
  database.close();
  return maps.slice(0, MAX_SAVED_MAPS);
}
