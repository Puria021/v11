import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { api, LogQuery, LogRow } from "../api/tauri";
import { Badge, Button, Card, ErrorBox, inputClass } from "../components/ui";
import { errorMessage, num } from "../lib/format";

const LEVEL_TONE: Record<string, "gray" | "amber" | "red" | "green"> = { DEBUG: "gray", INFO: "green", WARN: "amber", ERROR: "red" };

/// Plain-text report of the visible log rows, safe to paste into a support request (no keys or tokens are logged).
function copyReport(items: LogRow[], done: (v: boolean) => void) {
  const lines = items.slice(0, 200).map((r) => `${r.at} | ${r.level} | ${r.projectName ?? "-"} | ${r.action} | ${r.status} | ${r.message ?? ""}`);
  const text = ["SEO Master — گزارش خطا", `تعداد: ${items.length}`, ...lines].join("\n");
  navigator.clipboard.writeText(text).then(() => { done(true); setTimeout(() => done(false), 2500); }).catch(() => done(false));
}

export default function Logs() {
  const [q, setQ] = useState<LogQuery>({ page: 1, pageSize: 100 });
  const set = (patch: Partial<LogQuery>) => setQ((s) => ({ ...s, ...patch, page: patch.page ?? 1 }));
  const projects = useQuery({ queryKey: ["projects"], queryFn: api.listProjects });
  const campaigns = useQuery({ queryKey: ["campaigns", q.projectId], queryFn: () => api.listCampaigns(q.projectId || undefined) });
  const actions = useQuery({ queryKey: ["logActions"], queryFn: api.listLogActions });
  const logs = useQuery({ queryKey: ["logs", q], queryFn: () => api.listLogs(q), refetchInterval: 5000 });
  const [exported, setExported] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const exp = useMutation({ mutationFn: () => api.exportLogs(q), onSuccess: (p) => { setExported(p); setError(null); }, onError: (e) => setError(errorMessage(e)) });

  const d = logs.data;
  const pages = d ? Math.max(1, Math.ceil(d.total / d.pageSize)) : 1;
  const sel = (value: string | undefined, onChange: (v: string) => void, first: string, opts: [string, string][]) => (
    <select className={`${inputClass} w-44`} value={value ?? ""} onChange={(e) => onChange(e.target.value)}>
      <option value="">{first}</option>
      {opts.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
    </select>
  );

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">گزارش‌ها (Logs)</h1>
        <div className="flex gap-2">
          <Button variant="secondary" disabled={!logs.data?.items.length} onClick={() => copyReport(logs.data?.items ?? [], setCopied)}>
            {copied ? "گزارش کپی شد ✓" : "کپی گزارش برای پشتیبانی"}
          </Button>
          <Button variant="secondary" disabled={exp.isPending} onClick={() => exp.mutate()}>خروجی CSV</Button>
        </div>
      </div>
      {exported && <Card><p className="text-sm">فایل ذخیره شد: <span className="ltr inline-block break-all text-xs">{exported}</span></p></Card>}
      <ErrorBox message={error} />
      <Card className="flex flex-wrap items-center gap-2">
        {sel(q.projectId, (v) => set({ projectId: v || undefined, campaignId: undefined }), "همه‌ی پروژه‌ها", (projects.data ?? []).map((p) => [p.id, p.name]))}
        {sel(q.campaignId, (v) => set({ campaignId: v || undefined }), "همه‌ی کمپین‌ها", (campaigns.data ?? []).map((c) => [c.id, c.name]))}
        <input className={`${inputClass} ltr w-28`} placeholder="مقاله #" value={q.articleNo ?? ""} onChange={(e) => set({ articleNo: e.target.value ? Number(e.target.value.replace(/\D/g, "")) : undefined })} />
        {sel(q.level, (v) => set({ level: v || undefined }), "همه‌ی سطح‌ها", [["INFO", "INFO"], ["WARN", "WARN"], ["ERROR", "ERROR"]])}
        {sel(q.status, (v) => set({ status: v || undefined }), "همه‌ی وضعیت‌ها", [["SUCCESS", "SUCCESS"], ["FAILED", "FAILED"], ["WARN", "WARN"], ["INFO", "INFO"]])}
        {sel(q.action, (v) => set({ action: v || undefined }), "همه‌ی عملیات", (actions.data ?? []).map((a) => [a, a]))}
        <input type="date" className={`${inputClass} ltr w-40`} value={q.from ?? ""} onChange={(e) => set({ from: e.target.value || undefined })} />
        <input type="date" className={`${inputClass} ltr w-40`} value={q.to ?? ""} onChange={(e) => set({ to: e.target.value || undefined })} />
        <input className={`${inputClass} w-56`} placeholder="جستجو در پیام…" value={q.search ?? ""} onChange={(e) => set({ search: e.target.value || undefined })} />
      </Card>

      <div className="overflow-auto rounded-xl border border-slate-200 bg-white">
        <table className="w-full text-sm">
          <thead className="bg-slate-50 text-slate-600">
            <tr><th className="px-3 py-2 text-start">زمان</th><th className="px-3 py-2 text-start">سطح</th><th className="px-3 py-2 text-start">پروژه / کمپین</th><th className="px-3 py-2 text-start">مقاله</th><th className="px-3 py-2 text-start">عملیات</th><th className="px-3 py-2 text-start">وضعیت</th><th className="px-3 py-2 text-start">پیام</th></tr>
          </thead>
          <tbody>
            {d?.items.map((l) => (
              <tr key={l.id} className="border-t border-slate-100 align-top">
                <td className="ltr whitespace-nowrap px-3 py-2 text-xs text-slate-500">{l.at.replace("T", " ").slice(0, 19)}</td>
                <td className="px-3 py-2"><Badge tone={LEVEL_TONE[l.level] ?? "gray"}>{l.level}</Badge></td>
                <td className="px-3 py-2 text-xs">{l.projectName ?? "—"}<div className="text-slate-400">{l.campaignName ?? ""}</div></td>
                <td className="px-3 py-2">{l.articleNo ? num(l.articleNo) : "—"}</td>
                <td className="ltr px-3 py-2 text-xs">{l.action}</td>
                <td className="px-3 py-2 text-xs">{l.status}</td>
                <td className="px-3 py-2 text-xs text-slate-700">{l.message}</td>
              </tr>
            ))}
            {d?.items.length === 0 && <tr><td colSpan={7} className="px-3 py-8 text-center text-slate-500">گزارشی با این فیلترها پیدا نشد.</td></tr>}
          </tbody>
        </table>
      </div>
      <div className="flex items-center justify-between text-sm text-slate-600">
        <span>{num(d?.total ?? 0)} رکورد</span>
        <div className="flex items-center gap-2">
          <Button variant="secondary" disabled={(q.page ?? 1) <= 1} onClick={() => set({ page: (q.page ?? 1) - 1 })}>قبلی</Button>
          <span>{num(q.page ?? 1)} / {num(pages)}</span>
          <Button variant="secondary" disabled={(q.page ?? 1) >= pages} onClick={() => set({ page: (q.page ?? 1) + 1 })}>بعدی</Button>
        </div>
      </div>
    </div>
  );
}
