/** Shared viewport types (kept dependency-light for tests). */

export interface TilePixels {
  width: number;
  height: number;
  rgba: Uint8Array;
}
