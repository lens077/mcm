import { useCallback, useEffect, useRef, useState } from "react";
import type { ElementRef } from "../ipc/types";
import type { SceneGraph } from "../canvas/scene-types";
import { refKey } from "../canvas/scene-types";
import { hitEdge, hitNode } from "../canvas/hit";
import {
  DEFAULT_VIEWPORT,
  type Viewport,
  fitToBounds,
  panBy,
  renderScene,
  screenToWorld,
  zoomAt,
} from "../canvas/renderer";
import { centreOn, findSelectedNode, isOffscreen } from "../app/selection";
import type { Point } from "./interaction-model";

/** Optional gesture overrides supplied by a view's interaction hook. */
export interface Gestures {
  /** Returns true to claim the press (suppressing pan). */
  onPressStart?: (world: Point) => boolean;
  onPressMove?: (world: Point) => void;
  onPressEnd?: (world: Point) => void;
  onDoubleClick?: (world: Point) => void;
  /** Returns true when the click was fully handled. */
  onClick?: (world: Point) => boolean;
  /** Returns true when the key was consumed. */
  onKeyDown?: (event: KeyboardEvent) => boolean;
}

interface Props {
  scene: SceneGraph;
  selected: ElementRef | null;
  onSelect: (target: ElementRef | null) => void;
  label: string;
  gestures?: Gestures;
}

/**
 * Shared canvas surface: pan, zoom, hit-testing and rendering are identical for
 * every view because each view is just a different scene projection.
 */
export function SceneView({ scene, selected, onSelect, label, gestures }: Props) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [viewport, setViewport] = useState<Viewport>(DEFAULT_VIEWPORT);
  const panRef = useRef<{ x: number; y: number } | null>(null);
  const claimedRef = useRef(false);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || scene.nodes.length === 0) return;
    const rect = canvas.getBoundingClientRect();
    setViewport(fitToBounds(scene, rect.width, rect.height));
  }, [scene]);

  // Cross-view linkage (FR-007): re-locate the selection when it is off-screen.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const node = findSelectedNode(scene, selected);
    if (!node) return;
    const rect = canvas.getBoundingClientRect();
    setViewport((prev) => {
      if (!isOffscreen(node, prev, rect.width, rect.height)) return prev;
      return centreOn(node, rect.width, rect.height, prev.scale);
    });
  }, [scene, selected]);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    renderScene(canvas, scene, {
      viewport,
      selectedKey: selected ? refKey(selected) : undefined,
    });
  }, [scene, viewport, selected]);

  useEffect(() => {
    draw();
  }, [draw]);

  useEffect(() => {
    const onResize = () => {
      draw();
    };
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("resize", onResize);
    };
  }, [draw]);

  // Keyboard gestures only fire while this view has focus.
  useEffect(() => {
    const handler = gestures?.onKeyDown;
    if (!handler) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (document.activeElement !== canvas) return;
      if (handler(event)) event.preventDefault();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [gestures]);

  const localPoint = (event: React.MouseEvent<HTMLCanvasElement>): Point => {
    const rect = event.currentTarget.getBoundingClientRect();
    return { x: event.clientX - rect.left, y: event.clientY - rect.top };
  };

  return (
    <canvas
      ref={canvasRef}
      className="view-canvas"
      aria-label={label}
      tabIndex={0}
      onMouseDown={(event) => {
        const world = screenToWorld(viewport, localPoint(event));
        claimedRef.current = gestures?.onPressStart?.(world) ?? false;
        if (!claimedRef.current) panRef.current = localPoint(event);
      }}
      onMouseMove={(event) => {
        const local = localPoint(event);
        if (claimedRef.current) {
          gestures?.onPressMove?.(screenToWorld(viewport, local));
          return;
        }
        const origin = panRef.current;
        if (!origin) return;
        setViewport((prev) => panBy(prev, local.x - origin.x, local.y - origin.y));
        panRef.current = local;
      }}
      onMouseUp={(event) => {
        const local = localPoint(event);
        const world = screenToWorld(viewport, local);
        if (claimedRef.current) {
          claimedRef.current = false;
          gestures?.onPressEnd?.(world);
          return;
        }
        const origin = panRef.current;
        panRef.current = null;
        if (!origin) return;
        // A near-stationary press is a click, not a pan.
        if (Math.hypot(local.x - origin.x, local.y - origin.y) > 3) return;
        if (gestures?.onClick?.(world)) return;
        const node = hitNode(scene, world);
        if (node) {
          onSelect(node.ref);
          return;
        }
        const edge = hitEdge(scene, world);
        onSelect(edge ? edge.to : null);
      }}
      onDoubleClick={(event) => {
        gestures?.onDoubleClick?.(screenToWorld(viewport, localPoint(event)));
      }}
      onMouseLeave={() => {
        panRef.current = null;
        claimedRef.current = false;
      }}
      onWheel={(event) => {
        const factor = event.deltaY < 0 ? 1.1 : 1 / 1.1;
        const rect = event.currentTarget.getBoundingClientRect();
        setViewport((prev) =>
          zoomAt(prev, { x: event.clientX - rect.left, y: event.clientY - rect.top }, factor),
        );
      }}
    />
  );
}
