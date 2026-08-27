export type Theme = "light" | "dark";
const KEY = "mcm.theme";

export function readTheme(): Theme {
  const saved = localStorage.getItem(KEY);
  if (saved === "light" || saved === "dark") return saved;
  return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  localStorage.setItem(KEY, theme);
}

export function toggleTheme(): Theme {
  const next = readTheme() === "light" ? "dark" : "light";
  applyTheme(next);
  return next;
}
