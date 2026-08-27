import type { ElementRef, ValidationIssue } from "../ipc/types";
import { describeTarget, formatCyclePath } from "./format";

interface Props {
  issues: ValidationIssue[];
  onLocate: (target: ElementRef) => void;
}

export function IssuesPanel({ issues, onLocate }: Props) {
  const errors = issues.filter((issue) => issue.severity === "error");
  const warnings = issues.filter((issue) => issue.severity === "warning");

  return (
    <section className="panel issues-panel" aria-label="问题面板">
      <header className="panel-head">
        <h2>校验问题</h2>
        <span className="counts">
          <span className="count-error">{errors.length} 错误</span>
          <span className="count-warning">{warnings.length} 警告</span>
        </span>
      </header>

      {issues.length === 0 ? (
        <p className="empty-hint">校验通过，没有发现问题。</p>
      ) : (
        <ul className="issue-list">
          {[...errors, ...warnings].map((issue, index) => {
            const cycle = formatCyclePath(issue.cycle_path);
            return (
              <li key={`${issue.code}-${index}`} className={`issue issue-${issue.severity}`}>
                <button
                  type="button"
                  className="issue-locate"
                  onClick={() => {
                    onLocate(issue.target);
                  }}
                >
                  <span className="code">{issue.code}</span>
                  <span className="target">{describeTarget(issue.target)}</span>
                </button>
                <p className="message">{issue.message}</p>
                {cycle && <p className="cycle">环路径：{cycle}</p>}
                <p className="hint">修复建议：{issue.fix_hint}</p>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
