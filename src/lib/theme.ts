export type Theme = "light" | "dark" | "system";

const STORAGE_KEY = "seoMaster.theme";
const DARK_QUERY = "(prefers-color-scheme: dark)";

function systemPrefersDark(): boolean {
  try {
    return typeof window !== "undefined" && !!window.matchMedia && window.matchMedia(DARK_QUERY).matches;
  } catch {
    return false;
  }
}

export function getTheme(): Theme {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* storage may be unavailable */
  }
  return "system";
}

export function applyTheme(theme: Theme = getTheme()): void {
  const dark = theme === "dark" || (theme === "system" && systemPrefersDark());
  const root = document.documentElement;
  root.classList.toggle("dark", dark);
  root.style.colorScheme = dark ? "dark" : "light";
}

export function setTheme(theme: Theme): void {
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    /* storage may be unavailable */
  }
  applyTheme(theme);
}

let listening = false;

/** Applies the saved theme and keeps "system" in sync with the OS setting. Call once before render. */
export function initTheme(): void {
  applyTheme();
  if (listening) return;
  listening = true;
  try {
    const mq = window.matchMedia(DARK_QUERY);
    const onChange = () => {
      if (getTheme() === "system") applyTheme("system");
    };
    if (mq.addEventListener) mq.addEventListener("change", onChange);
    else mq.addListener(onChange);
  } catch {
    /* matchMedia unavailable */
  }
}
