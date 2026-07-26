/**
 * Riply State Management
 * 
 * Simple pub/sub store for managing application state.
 * Components subscribe to state changes and re-render automatically.
 */

class Store {
  constructor(initialState = {}) {
    this._state = initialState;
    this._listeners = new Map();
    this._nextId = 0;
  }

  /**
   * Get current state (read-only snapshot)
   */
  get state() {
    return this._state;
  }

  /**
   * Update state (shallow merge)
   */
  set(updates) {
    const prev = this._state;
    this._state = { ...this._state, ...updates };
    this._notify(prev);
  }

  /**
   * Subscribe to state changes
   * Returns unsubscribe function
   */
  subscribe(listener) {
    const id = this._nextId++;
    this._listeners.set(id, listener);
    
    return () => {
      this._listeners.delete(id);
    };
  }

  /**
   * Subscribe to a specific key
   */
  on(key, listener) {
    return this.subscribe((state, prev) => {
      if (state[key] !== prev[key]) {
        listener(state[key], prev[key], state);
      }
    });
  }

  _notify(prev) {
    for (const listener of this._listeners.values()) {
      try {
        listener(this._state, prev);
      } catch (error) {
        console.error('Store listener error:', error);
      }
    }
  }
}

// ── Application Store ───────────────────────────────────────

const initialState = {
  // Connection
  connected: false,
  error: null,
  
  // Files
  files: [],
  filesTotal: 0,
  filesLoading: false,
  
  // Review
  reviewQueue: [],
  reviewLoading: false,
  
  // Drives
  drives: [],
  drivesLoading: false,
  
  // Move
  moveStatus: { confirmed_count: 0, moved_count: 0 },
  
  // Config
  config: null,
  
  // SSE events
  lastEvent: null,
};

export const store = new Store(initialState);

export default store;
