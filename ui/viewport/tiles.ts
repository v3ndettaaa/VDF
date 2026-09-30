/**
 * Tile fetching: vdf-tile:// → raw RGBA (16-byte header + pixels).
 * Fetched only for keys the poll reports ready; failures are logged once
 * and the tile is retried on the next ready report.
 */

import { tileUrl } from "../app/ipc";
import type { TilePixels } from "./compositor-helpers";

const inflight = new Set<string>();
const failedOnce = new Set<string>();

export interface TileHeader {
  version: number;
  width: number;
  height: number;
}

export function parseTileHeader(buf: ArrayBuffer): { header: TileHeader; pixels: Uint8Array } {
  const view = new DataView(buf);
  const magic = String.fromCharCode(
    new Uint8Array(buf, 0, 4)[0]!,
    new Uint8Array(buf, 1, 4)[1]!,
    new Uint8Array(buf, 2, 4)[2]!,
    new Uint8Array(buf, 3, 4)[3]!,
  );
  if (magic !== "VDFT") throw new Error("bad tile magic");
  const version = view.getUint8(4);
  const width = view.getUint16(6, true);
  const height = view.getUint16(8, true);
  const pixels = new Uint8Array(buf, 16);
  if (pixels.length < width * height * 4) throw new Error("truncated tile");
  return { header: { version, width, height }, pixels };
}

export async function fetchTile(docId: number, key: string): Promise<TilePixels | null> {
  if (inflight.has(key) || failedOnce.has(key)) return null;
  inflight.add(key);
  try {
    const resp = await fetch(tileUrl(docId, key));
    if (!resp.ok) {
      if (!failedOnce.has(key)) {
        failedOnce.add(key);
        setTimeout(() => failedOnce.delete(key), 2000);
      }
      return null;
    }
    const buf = await resp.arrayBuffer();
    const { header, pixels } = parseTileHeader(buf);
    // copy: the underlying buffer is consumed once
    return {
      width: header.width,
      height: header.height,
      rgba: new Uint8Array(pixels),
    };
  } catch {
    return null;
  } finally {
    inflight.delete(key);
  }
}
