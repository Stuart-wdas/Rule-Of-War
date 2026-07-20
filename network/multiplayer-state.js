/** Creates transient client-side multiplayer connection state. */
export function createEmptyMultiplayerState() {
  return {
    socket: null,
    code: null,
    token: null,
    connected: false,
    opponentConnected: false,
    localReady: false,
    remoteReady: false,
    synchronizing: false,
    sequence: 0,
    reconnecting: false,
    pendingRequest: null,
  };
}

export function websocketAddress(locationLike = window.location) {
  const protocol = locationLike.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${locationLike.host}/ws`;
}
