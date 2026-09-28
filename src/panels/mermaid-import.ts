// Mermaid import flow for the import dialog, loaded on demand together with
// marked and mermaid. The webview parses and renders with the mermaid
// library; the core maps the parsed graph to a plan (宪法 IV), exactly as it
// does for screenshots and archify HTML.
import { ipc } from "../ipc/client";
import type { DiagramImport } from "../ipc/types";
import { blockLabel, extractMermaid, type MermaidBlock } from "./mermaid-source";
import { mermaidError, parseMermaid, renderMermaid } from "./mermaid-runtime";

export interface MermaidFile {
  /** File name without extension: the last-resort plan title. */
  name: string;
  blocks: MermaidBlock[];
  /** Picker label per block. */
  labels: string[];
  /** Front matter title or Markdown level-1 heading, if any. */
  title: string | null;
  /** Diagrams found but not readable, to show next to the picker. */
  skipped: string[];
}

/**
 * Read a .md / .mdx / .mmd file and list its diagrams.
 *
 * @throws a user-facing message when the file holds no Mermaid diagram.
 */
export async function readMermaidFile(path: string): Promise<MermaidFile> {
  const source = await ipc.diagramSourceRead(path);
  const doc = extractMermaid(source.text, path);
  if (doc.blocks.length === 0) {
    const why = doc.skipped.length > 0 ? `（${doc.skipped.join("；")}）` : "";
    throw new Error(
      `文件里没有可读取的 Mermaid 图${why}。图需写在 \`\`\`mermaid 代码块里，MDX 中也可以用 <Mermaid chart="…" />。`,
    );
  }
  return {
    name: source.name,
    blocks: doc.blocks,
    labels: doc.blocks.map(blockLabel),
    title: doc.title,
    skipped: doc.skipped,
  };
}

export interface MermaidOutcome {
  /** Rendered preview; null when mermaid could not draw it. */
  svg: string | null;
  /** Outline and report, for diagram types a plan can hold. */
  result: DiagramImport | null;
  /** Why there is no preview or no outline, in plain language. */
  error: string | null;
}

/** Render one diagram and, for flowcharts, convert it to outline text. */
export async function importMermaidBlock(
  file: MermaidFile,
  index: number,
): Promise<MermaidOutcome> {
  const block = file.blocks[index];
  if (!block) return { svg: null, result: null, error: "没有这张图" };
  const title = block.title ?? block.heading ?? file.title ?? "";

  let svg: string | null = null;
  try {
    svg = await renderMermaid(block.code);
  } catch (raw) {
    return { svg: null, result: null, error: `Mermaid 语法有误：${mermaidError(raw)}` };
  }

  try {
    const parsed = await parseMermaid(block.code, title);
    if (!parsed.graph) {
      return {
        svg,
        result: null,
        error: `这是 ${block.kind || parsed.type} 图，只能预览。目前只有 flowchart / graph 可以转为大纲。`,
      };
    }
    const result = await ipc.graphImport(parsed.graph, file.name);
    return { svg, result, error: null };
  } catch (raw) {
    const message = raw instanceof Object && "message" in raw ? String(raw.message) : String(raw);
    return { svg, result: null, error: message };
  }
}
