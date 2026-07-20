/** Local persistence for resumable human-match references. */
export class SavedMatchStore {
  constructor(storageKey, storage = window.localStorage) {
    this.storageKey = storageKey;
    this.storage = storage;
  }

  load() {
    try {
      const parsed = JSON.parse(this.storage.getItem(this.storageKey));
      if (!Array.isArray(parsed)) return [];
      return parsed.filter(
        (item) =>
          item &&
          /^[A-Z0-9]{6}$/.test(item.code) &&
          /^[A-Za-z0-9_-]{20,}$/.test(item.token) &&
          (item.owner === 0 || item.owner === 1),
      );
    } catch {
      return [];
    }
  }

  upsert(reference, limit = 12) {
    const saved = this.load().filter(
      (item) =>
        item.code !== reference.code || item.token !== reference.token,
    );
    saved.unshift(reference);
    this.write(saved.slice(0, limit));
  }

  remove(code, token) {
    this.write(
      this.load().filter(
        (item) => item.code !== code || item.token !== token,
      ),
    );
  }

  write(matches) {
    try {
      this.storage.setItem(this.storageKey, JSON.stringify(matches));
      return true;
    } catch {
      return false;
    }
  }
}
