import type { BadgeKind, SceneGraph, StyleRole } from "./scene-types";
import { refKey } from "./scene-types";
import type { Point } from "./hit";

export interface Viewport {
  /** World units per CSS pixel. */
  scale: number;
  offsetX: number;
  offsetY: number;
}

export const DEFAULT_VIEWPORT: Viewport = { scale: 1, offsetX: 0, offsetY: 0 };

export const MIN_SCALE = 0.1;
export const MAX_SCALE = 4;

export function clampScale(scale: number): number {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale));
}

export function screenToWorld(viewport: Viewport, point: Point): Point {
  return {
    x: (point.x - viewport.offsetX) / viewport.scale,
    y: (point.y - viewport.offsetY) / viewport.scale,
  };
}

export function worldToScreen(viewport: Viewport, point: Point): Point {
  return {
    x: point.x * viewport.scale + viewport.offsetX,
    y: point.y * viewport.scale + viewport.offsetY,
  };
}

/** Zooms around an anchor so the world point under the cursor stays put. */
export function zoomAt(viewport: Viewport, anchor: Point, factor: number): Viewport {
  const scale = clampScale(viewport.scale * factor);
  const worldBefore = screenToWorld(viewport, anchor);
  return {
    scale,
    offsetX: anchor.x - worldBefore.x * scale,
    offsetY: anchor.y - worldBefore.y * scale,
  };
}

export function panBy(viewport: Viewport, dx: number, dy: number): Viewport {
  return { ...viewport, offsetX: viewport.offsetX + dx, offsetY: viewport.offsetY + dy };
}

/** Fits scene bounds into the given viewport size with padding. */
export function fitToBounds(scene: SceneGraph, width: number, height: number, pad = 48): Viewport {
  const contentW = Math.max(1, scene.bounds.max_x - scene.bounds.min_x);
  const contentH = Math.max(1, scene.bounds.max_y - scene.bounds.min_y);
  const scale = clampScale(
    Math.min((width - pad * 2) / contentW, (height - pad * 2) / contentH, MAX_SCALE),
  );
  // 等比缩放后，较短的那一轴会有富余；把内容居中而不是顶到左上角。
  // 依赖网络这类宽扁内容尤其明显——不居中会孤零零贴在顶边。
  const slackX = Math.max(0, width - contentW * scale);
  const slackY = Math.max(0, height - contentH * scale);
  return {
    scale,
    offsetX: slackX / 2 - scene.bounds.min_x * scale,
    offsetY: slackY / 2 - scene.bounds.min_y * scale,
  };
}

/** Fill + stroke tokens for each semantic role. */
export function tokensFor(role: StyleRole): { fill: string; stroke: string } {
  switch (role) {
    case "task":
      return { fill: "--role-task-fill", stroke: "--role-task-stroke" };
    case "task_done":
      return { fill: "--role-task-done-fill", stroke: "--role-task-done-stroke" };
    case "task_error":
      return { fill: "--role-task-error-fill", stroke: "--role-task-error-stroke" };
    case "task_warning":
      return { fill: "--role-task-warning-fill", stroke: "--role-task-warning-stroke" };
    case "milestone":
      return { fill: "--role-milestone-fill", stroke: "--role-milestone-stroke" };
    case "hierarchy_edge":
      return { fill: "--role-hierarchy-edge", stroke: "--role-hierarchy-edge" };
    case "dependency_edge":
      return { fill: "--role-dependency-edge", stroke: "--role-dependency-edge" };
    case "milestone_edge":
      return { fill: "--role-milestone-edge", stroke: "--role-milestone-edge" };
  }
}

export function badgeToken(badge: BadgeKind): string {
  switch (badge) {
    case "error":
      return "--badge-error";
    case "warning":
      return "--badge-warning";
    case "done":
      return "--badge-done";
    case "milestone":
      return "--badge-milestone";
  }
}

/** Reads a CSS custom property, with a visible fallback if it is missing. */
export function readToken(root: HTMLElement, token: string): string {
  return getComputedStyle(root).getPropertyValue(token).trim() || "#888";
}

/** Resolves a semantic role to its themed fill color (never hard-coded). */
export function resolveRoleColor(root: HTMLElement, role: StyleRole): string {
  return readToken(root, tokensFor(role).fill);
}

export interface RenderOptions {
  viewport: Viewport;
  selectedKey?: string | undefined;
  reduceMotion?: boolean;
}

/** Sizes the backing store for the device pixel ratio and returns CSS size. */
export function prepareCanvas(
  canvas: HTMLCanvasElement,
  ctx: CanvasRenderingContext2D,
): { width: number; height: number } {
  const dpr = window.devicePixelRatio || 1;
  const rect = canvas.getBoundingClientRect();
  const width = Math.max(1, Math.floor(rect.width));
  const height = Math.max(1, Math.floor(rect.height));
  const backingW = Math.floor(width * dpr);
  const backingH = Math.floor(height * dpr);
  if (canvas.width !== backingW || canvas.height !== backingH) {
    canvas.width = backingW;
    canvas.height = backingH;
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  return { width, height };
}

/** Draws a scene graph. Rendering is a pure function of scene + viewport. */
export function renderScene(
  canvas: HTMLCanvasElement,
  scene: SceneGraph,
  options: RenderOptions,
): void {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const { width, height } = prepareCanvas(canvas, ctx);
  const root = document.documentElement;
  ctx.clearRect(0, 0, width, height);

  const { viewport } = options;
  const visibleMinX = -viewport.offsetX / viewport.scale;
  const visibleMinY = -viewport.offsetY / viewport.scale;
  const visibleMaxX = visibleMinX + width / viewport.scale;
  const visibleMaxY = visibleMinY + height / viewport.scale;

  ctx.save();
  ctx.setTransform(
    viewport.scale * (window.devicePixelRatio || 1),
    0,
    0,
    viewport.scale * (window.devicePixelRatio || 1),
    viewport.offsetX * (window.devicePixelRatio || 1),
    viewport.offsetY * (window.devicePixelRatio || 1),
  );

  ctx.lineWidth = 1.5;
  for (const edge of scene.edges) {
    if (edge.points.length < 4) continue;
    ctx.strokeStyle = resolveRoleColor(root, edge.style_role);
    ctx.beginPath();
    ctx.moveTo(edge.points[0] ?? 0, edge.points[1] ?? 0);
    for (let i = 2; i + 1 < edge.points.length; i += 2) {
      ctx.lineTo(edge.points[i] ?? 0, edge.points[i + 1] ?? 0);
    }
    ctx.stroke();
  }

  const textColor = readToken(root, "--role-task-text");
  const mutedColor = readToken(root, "--ink-muted");
  const selectedStroke = readToken(root, "--role-selected-stroke");
  const selectedGlow = readToken(root, "--role-selected-glow");

  for (const node of scene.nodes) {
    // Viewport culling keeps large plans at 60fps.
    if (
      node.x + node.w < visibleMinX ||
      node.x > visibleMaxX ||
      node.y + node.h < visibleMinY ||
      node.y > visibleMaxY
    ) {
      continue;
    }
    const isSelected = options.selectedKey === refKey(node.ref);
    const roleTokens = tokensFor(node.style_role);

    if (isSelected) {
      ctx.save();
      ctx.shadowColor = selectedGlow;
      ctx.shadowBlur = 16;
    }
    ctx.fillStyle = readToken(root, roleTokens.fill);
    ctx.strokeStyle = isSelected ? selectedStroke : readToken(root, roleTokens.stroke);
    ctx.lineWidth = isSelected ? 2.5 : 1.25;
    ctx.beginPath();
    ctx.roundRect(node.x, node.y, node.w, node.h, 8);
    ctx.fill();
    ctx.stroke();
    if (isSelected) ctx.restore();

    const hasSub = Boolean(node.sub_text);
    ctx.fillStyle = textColor;
    ctx.font = "600 14px system-ui, sans-serif";
    ctx.textBaseline = hasSub ? "alphabetic" : "middle";
    const titleY = hasSub ? node.y + node.h / 2 - 2 : node.y + node.h / 2;
    ctx.fillText(node.text, node.x + 12, titleY, Math.max(16, node.w - 32));

    if (node.sub_text) {
      ctx.fillStyle = mutedColor;
      ctx.font = "11px ui-monospace, monospace";
      ctx.textBaseline = "top";
      ctx.fillText(node.sub_text, node.x + 12, node.y + node.h / 2 + 4, Math.max(16, node.w - 24));
    }

    // Badges stack from the top-right corner, one dot per kind.
    node.badges.forEach((badge, index) => {
      ctx.fillStyle = readToken(root, badgeToken(badge));
      ctx.beginPath();
      ctx.arc(node.x + node.w - 10 - index * 11, node.y + 10, 4, 0, Math.PI * 2);
      ctx.fill();
    });
  }
  ctx.restore();
}
