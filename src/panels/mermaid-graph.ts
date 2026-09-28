// Mermaid flowchart → GraphSpec. The flowchart is parsed by mermaid itself
// (see mermaid-runtime.ts); this module only reads the parser's database, so
// it is testable with a plain object standing in for it.
import { parseInline } from "marked";
import type { GraphSpec } from "../ipc/types";

/** The slice of mermaid's FlowDB this importer reads. */
export interface FlowDbLike {
  getVertices(): Map<string, { id: string; text?: string; labelType?: string }>;
  getEdges(): {
    start: string;
    end: string;
    type?: string;
    stroke?: string;
    text?: string;
    labelType?: string;
  }[];
  getSubGraphs(): { id: string; title: string; labelType?: string; nodes: string[] }[];
}

/** Diagram types (as mermaid reports them) whose structure can become a plan. */
export const FLOWCHART_TYPES = ["flowchart", "flowchart-v2", "flowchart-elk"];

/**
 * Mermaid escapes `#quot;`-style entities into private markers while parsing
 * and turns them back into HTML entities only when rendering.
 */
function restoreEntities(text: string): string {
  return text.replaceAll("ﬂ°°", "&#").replaceAll("ﬂ°", "&").replaceAll("¶ß", ";");
}

/** Font Awesome shorthands such as `fa:fa-car` render as icons, not words. */
const ICON = /\bfa[bklrs]?:fa-[\w-]+\s*/g;

/**
 * A label as the lines a reader sees: `<br>` and newlines split lines,
 * Markdown strings lose their emphasis markers, tags go, entities decode.
 */
export function labelLines(text: string | undefined, labelType?: string): string[] {
  if (!text) return [];
  let html = restoreEntities(text).replace(ICON, "");
  if (labelType === "markdown") {
    html = html
      .split("\n")
      .map((line) => parseInline(line, { async: false }))
      .join("<br>");
  }
  html = html.replace(/<br\s*\/?>/gi, "\n");
  const doc = new DOMParser().parseFromString(`<body>${html}</body>`, "text/html");
  return (doc.body.textContent ?? "")
    .split("\n")
    .map((line) => line.replace(/\s+/g, " ").trim())
    .filter(Boolean);
}

function oneLine(text: string | undefined, labelType?: string): string {
  return labelLines(text, labelType).join(" ");
}

/** Arrow heads that point one way; `<-->` and `---` do not. */
function isDirected(type: string | undefined): boolean {
  return type !== undefined && type.startsWith("arrow_") && type !== "arrow_open";
}

export function flowToGraph(db: FlowDbLike, title: string): GraphSpec {
  const nodes = [...db.getVertices().values()].map((vertex) => {
    const [label = "", ...detail] = labelLines(vertex.text, vertex.labelType);
    return { id: vertex.id, label, detail };
  });
  const groups = db.getSubGraphs().map((sub) => ({
    id: sub.id,
    label: oneLine(sub.title, sub.labelType),
    members: [...sub.nodes],
  }));
  const edges = db
    .getEdges()
    // `~~~` links only nudge the layout; they relate nothing.
    .filter((edge) => edge.stroke !== "invisible")
    .map((edge) => ({
      from: edge.start,
      to: edge.end,
      directed: isDirected(edge.type),
      label: oneLine(edge.text, edge.labelType) || null,
    }));
  return { title, nodes, groups, edges };
}
