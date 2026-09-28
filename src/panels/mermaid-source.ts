// Finding Mermaid diagrams in Markdown, MDX or Mermaid files. Pure text work,
// so it is testable without the mermaid runtime.
//
// Markdown and MDX are parsed into an mdast tree with micromark (the parser
// behind MDX itself) rather than scanned for fences by hand: CommonMark fence
// rules, YAML front matter, and MDX's JSX / ESM / expressions all come from
// the reference implementation. MDX is parsed as MDX — Markdown inside JSX
// children counts, and a `{` or `<` in prose is an error, exactly as when the
// site that owns the file compiles it.
import type { Program } from "estree";
import type { Nodes, Parents, Root } from "mdast";
import { fromMarkdown } from "mdast-util-from-markdown";
import { frontmatterFromMarkdown } from "mdast-util-frontmatter";
import { mdxFromMarkdown, type MdxJsxFlowElement, type MdxJsxTextElement } from "mdast-util-mdx";
import { toString } from "mdast-util-to-string";
import { frontmatter } from "micromark-extension-frontmatter";
import { mdxjs } from "micromark-extension-mdxjs";
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
  /** Document front matter `title`, else the first level-1 heading. */
  title: string | null;
  /** Diagrams seen but not readable (e.g. a computed MDX prop), for the user. */
  skipped: string[];
}

const FRONT_MATTER = /^\s*---\r?\n([\s\S]*?)\r?\n---\s*(?:\r?\n|$)/;

/** Split off a leading `---` YAML block, which Mermaid allows for config. */
function splitFrontMatter(code: string): { yaml: string; body: string } {
  const match = FRONT_MATTER.exec(code);
  if (!match) return { yaml: "", body: code };
  return { yaml: match[1] ?? "", body: code.slice(match[0].length) };
}

/** Top-level scalar `title:` of a YAML block. */
function yamlTitle(yaml: string): string | null {
  for (const line of yaml.split(/\r?\n/)) {
    const match = /^title:\s*(.*?)\s*$/.exec(line);
    if (!match) continue;
    const value = (match[1] ?? "").replace(/^(["'])(.*)\1$/, "$2").trim();
    return value || null;
  }
  return null;
}

/** `title:` from a diagram's front matter; only the top-level scalar form. */
export function frontMatterTitle(code: string): string | null {
  return yamlTitle(splitFrontMatter(code).yaml);
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

/** MDX components that take Mermaid source as a prop, and those props. */
const MERMAID_COMPONENTS = new Set(["Mermaid", "MermaidDiagram"]);
const SOURCE_PROPS = ["chart", "value", "code", "definition"];

/**
 * The static string an attribute holds: `chart="…"`, `chart={"…"}` or
 * `` chart={`…`} `` without `${}`. Null for anything computed at runtime.
 * Read from the compiled expression, not the raw text: MDX drops
 * line-leading whitespace inside expressions, and the component on the site
 * receives the string without it — so does the importer.
 */
function staticProp(element: MdxJsxFlowElement | MdxJsxTextElement): string | null {
  for (const attr of element.attributes) {
    if (attr.type !== "mdxJsxAttribute" || !SOURCE_PROPS.includes(attr.name)) continue;
    const value = attr.value;
    if (typeof value === "string") return value;
    if (!value) return null;
    const program = value.data?.estree as Program | null | undefined;
    const statement = program?.body[0];
    if (program?.body.length !== 1 || statement?.type !== "ExpressionStatement") return null;
    const expr = statement.expression;
    if (expr.type === "Literal" && typeof expr.value === "string") return expr.value;
    if (expr.type === "TemplateLiteral" && expr.expressions.length === 0) {
      return expr.quasis[0]?.value.cooked ?? null;
    }
    return null;
  }
  return null;
}

/**
 * Text a reader sees in a heading. `{…}` expressions are code evaluated at
 * build time (comments, variables), not text, so they are left out.
 */
function headingText(node: Nodes): string {
  if (node.type === "mdxTextExpression" || node.type === "mdxFlowExpression") return "";
  if ("children" in node) return (node as Parents).children.map(headingText).join("");
  return toString(node);
}

/** Where a parse error happened, in the dialog's words. */
function parseFailure(raw: unknown, format: string): Error {
  const error = raw as { reason?: string; message?: string; line?: number; column?: number };
  const where =
    typeof error.line === "number"
      ? `第 ${String(error.line)} 行第 ${String(error.column ?? 1)} 列：`
      : "";
  return new Error(`${format} 解析失败，${where}${error.reason ?? error.message ?? String(raw)}`);
}

function parseTree(text: string, mdx: boolean): Root {
  try {
    return fromMarkdown(text, {
      extensions: mdx ? [frontmatter(), mdxjs()] : [frontmatter()],
      mdastExtensions: mdx
        ? [frontmatterFromMarkdown(), mdxFromMarkdown()]
        : [frontmatterFromMarkdown()],
    });
  } catch (raw) {
    throw parseFailure(raw, mdx ? "MDX" : "Markdown");
  }
}

/**
 * Every Mermaid diagram in `text`. A `.mmd` / `.mermaid` file is one
 * diagram. Markdown contributes each ```mermaid fenced block; MDX also
 * `<Mermaid chart="…" />` style components.
 *
 * @throws when an MDX file is not valid MDX (the error names line and column).
 */
export function extractMermaid(text: string, path: string): MermaidDocument {
  const ext = extensionOf(path);
  if (ext === "mmd" || ext === "mermaid") {
    return { blocks: text.trim() ? [block(text, null)] : [], title: null, skipped: [] };
  }
  const tree = parseTree(text, ext === "mdx");
  const blocks: MermaidBlock[] = [];
  const skipped: string[] = [];
  let matterTitle: string | null = null;
  let firstH1: string | null = null;
  let heading: string | null = null;

  // Pre-order is document order, so `heading` is always the nearest above.
  const visit = (node: Nodes) => {
    switch (node.type) {
      case "yaml":
        matterTitle ??= yamlTitle(node.value);
        return;
      case "heading": {
        heading = headingText(node).replace(/\s+/g, " ").trim() || heading;
        if (node.depth === 1) firstH1 ??= heading;
        return;
      }
      case "code": {
        const lang = (node.lang ?? "").trim().toLowerCase();
        if (lang === "mermaid" && node.value.trim()) blocks.push(block(node.value, heading));
        return;
      }
      case "mdxJsxFlowElement":
      case "mdxJsxTextElement":
        if (node.name && MERMAID_COMPONENTS.has(node.name)) {
          const code = staticProp(node);
          if (code?.trim()) blocks.push(block(code, heading));
          else {
            const line = node.position ? `第 ${String(node.position.start.line)} 行 ` : "";
            skipped.push(`${line}<${node.name}> 的图源码是运行时表达式，无法静态读取，已跳过`);
          }
          return;
        }
        break;
      default:
        break;
    }
    if ("children" in node) for (const child of (node as Parents).children) visit(child);
  };
  visit(tree);
  return { blocks, title: matterTitle ?? firstH1, skipped };
}

/** One line per block for the picker, e.g. "图 2 · flowchart · 部署". */
export function blockLabel(block: MermaidBlock, index: number): string {
  const parts = [`图 ${String(index + 1)}`, block.kind || "未知类型"];
  const name = block.title ?? block.heading;
  if (name) parts.push(name);
  return parts.join(" · ");
}
