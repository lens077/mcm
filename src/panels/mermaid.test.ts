import { describe, expect, it } from "vitest";
import { blockLabel, extractMermaid, frontMatterTitle } from "./mermaid-source";
import { isMermaidPath } from "./mermaid-path";
import { labelLines } from "./mermaid-graph";
import { parseMermaid } from "./mermaid-runtime";

const MARKDOWN = `# 订单系统

说明文字。

## 部署

\`\`\`mermaid
flowchart LR
  a --> b
\`\`\`

\`\`\`ts
const x = 1;
\`\`\`

- 列表里的图：

  ~~~mermaid
  sequenceDiagram
    A->>B: hi
  ~~~
`;

describe("finding Mermaid in files", () => {
  it("routes Markdown and Mermaid files by extension", () => {
    expect(isMermaidPath("/a/设计.MD")).toBe(true);
    expect(isMermaidPath("C:\\x\\flow.mmd")).toBe(true);
    expect(isMermaidPath("/a/shot.png")).toBe(false);
    expect(isMermaidPath("/a/mermaid")).toBe(false);
  });

  it("takes every mermaid fence from Markdown, nested ones included", () => {
    const doc = extractMermaid(MARKDOWN, "x.md");
    expect(doc.title).toBe("订单系统");
    expect(doc.blocks.map((b) => b.kind)).toEqual(["flowchart", "sequenceDiagram"]);
    expect(doc.blocks[0]?.heading).toBe("部署");
    expect(doc.blocks[0]?.code).toBe("flowchart LR\n  a --> b");
  });

  it("treats a .mmd file as one diagram", () => {
    const code = "---\ntitle: '支付流程'\n---\n%% 注释\ngraph TD\n  a --> b\n";
    const doc = extractMermaid(code, "flow.mmd");
    expect(doc.blocks).toHaveLength(1);
    expect(doc.blocks[0]?.kind).toBe("graph");
    expect(doc.blocks[0]?.title).toBe("支付流程");
    expect(blockLabel(doc.blocks[0]!, 0)).toBe("图 1 · graph · 支付流程");
    expect(extractMermaid("  \n", "empty.mmd").blocks).toEqual([]);
  });

  it("reads document front matter instead of mistaking it for a heading", () => {
    const doc = extractMermaid(
      "---\ntitle: 设计文档\n---\n\n```mermaid\ngraph TD\n  a\n```\n",
      "x.md",
    );
    expect(doc.title).toBe("设计文档");
    expect(doc.blocks[0]?.heading).toBeNull();
  });

  it("reads MDX: fences inside JSX, and Mermaid components with literal source", () => {
    const mdx = `---
title: 架构说明
---
import Tabs from "@theme/Tabs";
import Mermaid from "@theme/Mermaid";
export const meta = { draft: false };

# 总览 {/* 注释 */}

<Tabs>
  <TabItem value="flow">
\`\`\`mermaid
flowchart LR
  a --> b
\`\`\`
  </TabItem>
</Tabs>

## 部署

<Mermaid chart={\`graph TD
  x --> y\`} />

<Mermaid value="sequenceDiagram
  A->>B: hi" />

<Mermaid chart={source} />
`;
    const doc = extractMermaid(mdx, "/docs/guide.mdx");
    expect(doc.title).toBe("架构说明");
    expect(doc.blocks.map((b) => [b.kind, b.heading])).toEqual([
      ["flowchart", "总览"],
      ["graph", "部署"],
      ["sequenceDiagram", "部署"],
    ]);
    // MDX drops line-leading whitespace inside expressions when it compiles,
    // so this is exactly the string the component receives on the site.
    expect(doc.blocks[1]?.code).toBe("graph TD\nx --> y");
    expect(doc.skipped).toEqual([
      "第 27 行 <Mermaid> 的图源码是运行时表达式，无法静态读取，已跳过",
    ]);
  });

  it("reports invalid MDX with its position", () => {
    expect(() => extractMermaid("# 标题\n\n价格 {a +} 元\n", "x.mdx")).toThrow(
      /MDX 解析失败，第 3 行/,
    );
    // The same text is fine as plain Markdown.
    expect(extractMermaid("# 标题\n\n价格 {a +} 元\n", "x.md").title).toBe("标题");
  });

  it("reads only the top-level front matter title", () => {
    expect(frontMatterTitle("---\nconfig:\n  title: no\n---\nflowchart")).toBeNull();
    expect(frontMatterTitle("flowchart LR\n title: no")).toBeNull();
  });
});

describe("label text", () => {
  it("splits on <br>, decodes entities and drops markup", () => {
    expect(labelLines("网关<br/>Nginx &amp; <b>TLS</b>", "string")).toEqual([
      "网关",
      "Nginx & TLS",
    ]);
    expect(labelLines("**加粗** 支付\n第二行", "markdown")).toEqual(["加粗 支付", "第二行"]);
    expect(labelLines("fa:fa-car 车辆")).toEqual(["车辆"]);
    expect(labelLines(undefined)).toEqual([]);
  });
});

describe("parsing with mermaid", () => {
  it("reads a flowchart's nodes, subgraphs and arrows", async () => {
    const code = `flowchart LR
  A["网关<br/>Nginx"] --> B(订单服务)
  B -- 写入 --> DB[(PostgreSQL)]
  subgraph core [核心域]
    B
    subgraph inner [内层]
      C{"\`**支付**\`"}
    end
  end
  C <--> D
  A ~~~ D
  A --> core
  B -.->|异步 #quot;事件#quot;| C`;
    const parsed = await parseMermaid(code, "订单");
    const graph = parsed.graph!;
    expect(graph.title).toBe("订单");
    expect(graph.nodes.find((n) => n.id === "A")).toEqual({
      id: "A",
      label: "网关",
      detail: ["Nginx"],
    });
    expect(graph.nodes.find((n) => n.id === "C")?.label).toBe("支付");
    expect(graph.groups).toEqual([
      { id: "inner", label: "内层", members: ["C"] },
      { id: "core", label: "核心域", members: ["B", "inner"] },
    ]);
    expect(graph.edges).toEqual([
      { from: "A", to: "B", directed: true, label: null },
      { from: "B", to: "DB", directed: true, label: "写入" },
      { from: "C", to: "D", directed: false, label: null },
      { from: "A", to: "core", directed: true, label: null },
      { from: "B", to: "C", directed: true, label: '异步 "事件"' },
    ]);
  });

  it("recognises other diagram types without converting them", async () => {
    const parsed = await parseMermaid("sequenceDiagram\n  A->>B: hi", "");
    expect(parsed.type).toBe("sequence");
    expect(parsed.graph).toBeNull();
  });

  it("rejects broken syntax", async () => {
    await expect(parseMermaid("flowchart LR\n  a -->", "")).rejects.toThrow();
  });
});
