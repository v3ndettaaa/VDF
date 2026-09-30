/**
 * Document viewport module: owns the canvas subtree and the controller.
 */

import { app } from "../state/app";
import { ViewportController } from "./controller";

export let controller: ViewportController | null = null;

export function initViewport(root: HTMLElement): void {
  controller = new ViewportController();
  controller.init(root, {
    onStats: (stats) => {
      app.setViewportStats(stats);
    },
    onLayout: (layout) => {
      app.setLayout(layout.pages.length, layout.doc_h);
    },
    onZoomChanged: (zoom) => {
      app.setZoomFromShell(zoom * 100);
    },
  });

  // Opening/closing documents flows through the store
  app.subscribe((s) => {
    if (s.activeDoc && controller && !controller.hasDocument(s.activeDoc.id)) {
      controller.openDocument(s.activeDoc.id);
    }
    if (!s.activeDoc && controller) {
      controller.closeDocument();
    }
  });
}
