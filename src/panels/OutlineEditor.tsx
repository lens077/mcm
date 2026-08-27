import { useMemo } from "react";
import type { ValidationIssue } from "../ipc/types";
import { parseErrorLines } from "./format";

interface Props {
  text: string;
  issues: ValidationIssue[];
  onChange: (text: string) => void;
}

/** The generate action lives in the toolbar so there is one entry point. */
export function OutlineEditor({ text, issues, onChange }: Props) {
  const errorLines = useMemo(() => parseErrorLines(issues), [issues]);
  const lines = useMemo(() => text.split("\n"), [text]);

  return (
    <section className="panel outline-editor" aria-label="大纲编辑器">
      <header className="panel-head">
        <h2>项目大纲</h2>
        <span className="line-count">{lines.length} 行</span>
      </header>

      <div className="editor-body">
        <ol className="gutter" aria-hidden="true">
          {lines.map((_, index) => {
            const lineNumber = index + 1;
            const issue = errorLines.get(lineNumber);
            return (
              <li
                key={lineNumber}
                className={issue ? "gutter-line gutter-error" : "gutter-line"}
                title={issue ? `${issue.code}: ${issue.message}` : undefined}
              >
                {lineNumber}
              </li>
            );
          })}
        </ol>
        <textarea
          className="editor-area"
          value={text}
          spellCheck={false}
          aria-label="项目大纲文本"
          onChange={(event) => {
            onChange(event.target.value);
          }}
        />
      </div>

      {errorLines.size > 0 && (
        <ul className="inline-errors">
          {[...errorLines.entries()].map(([line, issue]) => (
            <li key={line}>
              <span className="code">{issue.code}</span>
              <span className="line">第 {line} 行</span>
              <span>{issue.message}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
