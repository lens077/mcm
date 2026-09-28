// Which picked files go to the Mermaid importer. Kept apart from the parser
// modules so the dialog can decide without loading marked or mermaid.

/** Extensions routed to the Mermaid importer instead of image / HTML import. */
export const MERMAID_EXTENSIONS = ["md", "markdown", "mmd", "mermaid"] as const;

export function extensionOf(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? "";
  const dot = name.lastIndexOf(".");
  return dot < 0 ? "" : name.slice(dot + 1).toLowerCase();
}

export function isMermaidPath(path: string): boolean {
  return (MERMAID_EXTENSIONS as readonly string[]).includes(extensionOf(path));
}
