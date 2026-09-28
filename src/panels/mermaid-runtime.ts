// The mermaid library, loaded on first use: it is large and only the import
// dialog needs it, so it stays out of the startup bundle (冷启动预算).
import type { GraphSpec } from "../ipc/types";
import { FLOWCHART_TYPES, flowToGraph, type FlowDbLike } from "./mermaid-graph";

type Mermaid = (typeof import("mermaid"))["default"];

let loading: Promise<Mermaid> | null = null;

function load(): Promise<Mermaid> {
  loading ??= import("mermaid").then(({ default: mermaid }) => mermaid);
  return loading;
}

function currentTheme(): "dark" | "default" {
  return document.documentElement.dataset.theme === "dark" ? "dark" : "default";
}

let renders = 0;

/**
 * Render `code` to SVG markup. `strict` makes mermaid sanitise labels and
 * disables click handlers, so an imported file cannot run script.
 */
export async function renderMermaid(code: string): Promise<string> {
  const mermaid = await load();
  mermaid.initialize({ startOnLoad: false, securityLevel: "strict", theme: currentTheme() });
  renders += 1;
  const { svg } = await mermaid.render(`mcm-mermaid-${String(renders)}`, code);
  return svg;
}

export interface ParsedMermaid {
  /** Diagram type as mermaid names it, e.g. "flowchart-v2", "sequence". */
  type: string;
  /** The plan-ready structure; null for diagram types a plan cannot hold. */
  graph: GraphSpec | null;
}

/** Parse `code` with mermaid's own parser and read the flowchart structure. */
export async function parseMermaid(code: string, title: string): Promise<ParsedMermaid> {
  const mermaid = await load();
  mermaid.initialize({ startOnLoad: false, securityLevel: "strict" });
  const diagram = await mermaid.mermaidAPI.getDiagramFromText(code);
  if (!FLOWCHART_TYPES.includes(diagram.type)) return { type: diagram.type, graph: null };
  return { type: diagram.type, graph: flowToGraph(diagram.db as unknown as FlowDbLike, title) };
}

/** Mermaid errors carry the parser's message; keep only what a user can act on. */
export function mermaidError(raw: unknown): string {
  const text = raw instanceof Error ? raw.message : String(raw);
  return text.split("\n").slice(0, 4).join("\n");
}
