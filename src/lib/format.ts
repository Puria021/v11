import type { AppError } from "../api/tauri";

export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String((e as AppError).message);
  if (typeof e === "string") return e;
  return "خطای ناشناخته رخ داد.";
}

export function faDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (isNaN(d.getTime())) return "—";
  return d.toLocaleString("fa-IR", { dateStyle: "medium", timeStyle: "short" });
}

export const num = (n: number) => n.toLocaleString("fa-IR");
