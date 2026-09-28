// Finding Mermaid diagrams in a Markdown or Mermaid file. Pure text work, so
// it is testable without the mermaid runtime. Markdown is tokenised with
// `marked` (already a mermaid dependency) rather than a hand-rolled fence
// scanner, so indented, `~~~` and nested fences follow CommonMark.
import { lexer, walkTokens, type Tokens } from "marked";
import { extensionOf } from "./mermaid-path";

export interface MermaidBlock {
  /** Diagram source, without the fence. */
  code: string;
  /** First keyword, e.g. "flowchart" or "sequenceDiagram". */
  kind: string;
  /** Title from the diagram's front matter, if any. */
  title: string | null;
  /** Nearest Markdown heading above the block, if any. */
  heading: string | null;
}

export interface MermaidDocument {
  blocks: MermaidBlock[];
  /** First level-1 Markdown heading; null for bare Mermaid files. */
  title: string | null;
}

const FRONT_MATTER = /^\s*---\r?\n([\s\S]*?)\r?\n---\s*(?:\r?\n|$)/;

/** Split off a leading `---` YAML block, which Mermaid allows for config. */
function splitFrontMatter(code: string): { yaml: string; body: string } {
  const match = FRONT_MATTER.exec(code);
  if (!match) return { yaml: "", body: code };
  return { yaml: match[1] ?? "", body: code.slice(match[0].length) };
}

/** `title:` from the front matter; only the top-level scalar form. */
export function frontMatterTitle(code: string): string | null {
  const { yaml } = splitFrontMatter(code);
  for (const line of yaml.split(/\r?\n/)) {
    const match = /^title:\s*(.*?)\s*$/.exec(line);
    if (!match) continue;
    const value = (match[1] ?? "").replace(/^(["'])(.*)\1$/, "$2").trim();
    return value || null;
  }
  return null;
}

/** First keyword of the diagram, skipping front matter, `%%` comments and directives. */
export function diagramKind(code: string): string {
  const { body } = splitFrontMatter(code);
  for (const raw of body.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("%%")) continue;
    return /^[A-Za-z][\w-]*/.exec(line)?.[0] ?? "";
  }
  return "";
}

function block(code: string, heading: string | null): MermaidBlock {
  return { code, kind: diagramKind(code), title: frontMatterTitle(code), heading };
}

/**
 * Every Mermaid diagram in `text`. A `.mmd` / `.mermaid` file is one
 * diagram; Markdown contributes each ```mermaid fenced block.
 */
export function extractMermaid(text: string, path: string): MermaidDocument {
  const ext = extensionOf(path);
  if (ext === "mmd" || ext === "mermaid") {
    return { blocks: text.trim() ? [block(text, null)] : [], title: null };
  }
  const blocks: MermaidBlock[] = [];
  let title: string | null = null;
  let heading: string | null = null;
  walkTokens(lexer(text), (token) => {
    if (token.type === "heading") {
      const h = token as Tokens.Heading;
      heading = h.text.trim() || heading;
      if (h.depth === 1 && title === null) title = heading;
    } else if (token.type === "code") {
      const code = token as Tokens.Code;
      const lang = (code.lang ?? "").trim().split(/\s+/)[0]?.toLowerCase();
      if (lang === "mermaid" && code.text.trim()) blocks.push(block(code.text, heading));
    }
  });
  return { blocks, title };
}

/** One line per block for the picker, e.g. "图 2 · flowchart · 部署". */
export function blockLabel(block: MermaidBlock, index: number): string {
  const parts = [`图 ${String(index + 1)}`, block.kind || "未知类型"];
  const name = block.title ?? block.heading;
  if (name) parts.push(name);
  return parts.join(" · ");
}
