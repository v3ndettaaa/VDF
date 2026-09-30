/**
 * Minimal observable store (~150 LOC budget — MASTER_PLAN.md §9).
 * No framework, no external state library: an immutable state snapshot,
 * a Set of listeners, and that is all.
 */

export type Listener<T> = (state: T) => void;

export class Store<T extends object> {
  #state: T;
  #listeners = new Set<Listener<T>>();

  constructor(initial: T) {
    this.#state = Object.freeze({ ...initial }) as T;
  }

  /** Current immutable state snapshot. */
  get(): T {
    return this.#state;
  }

  /** Shallow-merge a patch and notify listeners (old snapshot stays valid). */
  set(patch: Partial<T>): void {
    this.#state = Object.freeze({ ...this.#state, ...patch }) as T;
    for (const listener of [...this.#listeners]) {
      listener(this.#state);
    }
  }

  /** Derive a patch from the current state. */
  update(fn: (state: T) => Partial<T>): void {
    this.set(fn(this.#state));
  }

  /** Subscribe to state changes; returns an unsubscribe function. */
  subscribe(listener: Listener<T>): () => void {
    this.#listeners.add(listener);
    return () => {
      this.#listeners.delete(listener);
    };
  }
}
