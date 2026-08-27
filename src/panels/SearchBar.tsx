import { useCallback, useEffect, useState } from "react";
import type { ElementRef, SearchMatch } from "../ipc/types";
import { hasTauri, ipc } from "../ipc/client";
import { nextIndex } from "./search-nav";

interface Props {
  /** Bumped whenever the plan changes, so stale results are refreshed. */
  revision: number;
  onLocate: (target: ElementRef) => void;
}

export function SearchBar({ revision, onLocate }: Props) {
  const [query, setQuery] = useState("");
  const [matches, setMatches] = useState<SearchMatch[]>([]);
  const [cursor, setCursor] = useState(0);

  const runSearch = useCallback(async (text: string) => {
    if (!hasTauri() || text.trim().length === 0) {
      setMatches([]);
      return;
    }
    const result = await ipc.search(text);
    setMatches(result.matches);
    setCursor(0);
    const first = result.matches[0];
    if (first) onLocate(first.ref);
  }, [onLocate]);

  // Re-run when the plan changed under us.
  useEffect(() => {
    if (query.trim().length > 0) void runSearch(query);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revision]);

  const step = (delta: number) => {
    if (matches.length === 0) return;
    const index = nextIndex(cursor, delta, matches.length);
    setCursor(index);
    const match = matches[index];
    if (match) onLocate(match.ref);
  };

  return (
    <div className="search-bar">
      <input
        type="search"
        className="search-input"
        placeholder="搜索任务、备注、负责人"
        aria-label="搜索任务"
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          void runSearch(event.target.value);
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            step(event.shiftKey ? -1 : 1);
          }
        }}
      />
      {query.trim().length > 0 && (
        <span className="search-count">
          {matches.length === 0 ? "无结果" : `${String(cursor + 1)}/${String(matches.length)}`}
        </span>
      )}
      <button
        type="button"
        className="search-step"
        aria-label="上一个匹配"
        disabled={matches.length === 0}
        onClick={() => {
          step(-1);
        }}
      >
        ↑
      </button>
      <button
        type="button"
        className="search-step"
        aria-label="下一个匹配"
        disabled={matches.length === 0}
        onClick={() => {
          step(1);
        }}
      >
        ↓
      </button>
    </div>
  );
}
