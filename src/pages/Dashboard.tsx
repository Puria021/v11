import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { api } from "../api/tauri";
import { Badge, Card } from "../components/ui";
import { faDate, num } from "../lib/format";

export default function Dashboard({ online }: { online: boolean | null }) {
  const stats = useQuery({ queryKey: ["stats"], queryFn: api.getDashboardStats });
  const projects = useQuery({ queryKey: ["projects"], queryFn: api.listProjects });
  const s = stats.data;

  const items: [string, number | undefined][] = [
    ["پروژه‌ها", s?.projects],
    ["کمپین‌ها", s?.campaigns],
    ["کلمات کلیدی بررسی‌شده", s?.keywordsResearched],
    ["کلمات کلیدی تأییدشده", s?.keywordsApproved],
    ["برنامه‌های مقاله", s?.articlePlans],
    ["آماده‌ی تولید", s?.articlesReady],
    ["در حال تولید", s?.articlesGenerating],
    ["پیش‌نویس ایجادشده در وردپرس", s?.draftsCreated],
    ["خطاها", s?.errors],
  ];

  return (
    <div className="space-y-8">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">داشبورد</h1>
        <div className="text-sm">
          اینترنت: {online ? <Badge tone="green">متصل ✓</Badge> : online === false ? <Badge tone="amber">آفلاین</Badge> : <Badge tone="gray">نامشخص</Badge>}
        </div>
      </div>

      <div className="grid grid-cols-2 gap-4 lg:grid-cols-3 xl:grid-cols-5">
        {items.map(([label, value]) => (
          <Card key={label}>
            <div className="text-2xl font-bold text-brand-700">{value === undefined ? "…" : num(value)}</div>
            <div className="mt-1 text-xs text-slate-500">{label}</div>
          </Card>
        ))}
      </div>

      <section>
        <h2 className="mb-3 text-lg font-semibold">پروژه‌ها</h2>
        {projects.data?.length === 0 && (
          <Card>
            <p className="text-sm text-slate-600">
              هنوز پروژه‌ای ندارید. <Link className="text-brand-600 underline" to="/projects">اولین پروژه را بسازید</Link>.
            </p>
          </Card>
        )}
        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
          {projects.data?.map((p) => (
            <Link key={p.id} to={`/projects/${p.id}`}>
              <Card className="transition hover:border-brand-500">
                <div className="flex items-start justify-between gap-2">
                  <div className="font-semibold">{p.name}</div>
                  <WpBadge status={p.wpStatus} />
                </div>
                <div className="ltr mt-1 truncate text-xs text-slate-500">{p.siteUrl}</div>
                <div className="mt-3 text-xs text-slate-500">آخرین فعالیت: {faDate(p.lastActivityAt ?? p.updatedAt)}</div>
              </Card>
            </Link>
          ))}
        </div>
      </section>
    </div>
  );
}

export function WpBadge({ status }: { status: "UNTESTED" | "CONNECTED" | "FAILED" | null }) {
  if (status === "CONNECTED") return <Badge tone="green">متصل ✓</Badge>;
  if (status === "FAILED") return <Badge tone="red">خطا در اتصال</Badge>;
  if (status === "UNTESTED") return <Badge tone="amber">تست‌نشده</Badge>;
  return <Badge tone="gray">بدون اتصال</Badge>;
}
