import { useQuery } from "@tanstack/react-query";
import { api, QuotaBucket } from "../api/tauri";
import { Badge, Card } from "../components/ui";
import { num } from "../lib/format";

export default function Dashboard({ online }: { online: boolean | null }) {
  const stats = useQuery({ queryKey: ["stats"], queryFn: api.getDashboardStats });
  const quota = useQuery({ queryKey: ["quota"], queryFn: api.getQuota });
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
        <h2 className="mb-3 text-lg font-semibold">سهمیه‌ی باقی‌مانده</h2>
        <Card className="grid gap-6 md:grid-cols-2">
          <QuotaRow label="مقاله‌های باقی‌مانده" bucket={quota.data?.articles} />
          <QuotaRow label="توضیحات محصول باقی‌مانده" bucket={quota.data?.products} />
        </Card>
      </section>
    </div>
  );
}

function QuotaRow({ label, bucket }: { label: string; bucket: QuotaBucket | undefined }) {
  const total = bucket?.total ?? 0;
  const remaining = bucket?.remaining ?? 0;
  const pct = total > 0 ? Math.min(100, Math.max(0, (remaining / total) * 100)) : 0;
  const empty = bucket !== undefined && remaining <= 0;
  return (
    <div className="space-y-2">
      <div className="flex items-baseline justify-between gap-3 text-sm">
        <span className="text-slate-600">{label}</span>
        <span className="font-bold text-brand-700">
          {bucket ? `${num(remaining)} از ${num(total)}` : "…"}
        </span>
      </div>
      <div
        className="quota-bar"
        role="progressbar"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={total}
        aria-valuenow={remaining}
      >
        <div
          className={`quota-bar__fill ${empty ? "quota-bar__fill--empty" : ""}`}
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}

export function WpBadge({ status }: { status: "UNTESTED" | "CONNECTED" | "FAILED" | null }) {
  if (status === "CONNECTED") return <Badge tone="green">متصل ✓</Badge>;
  if (status === "FAILED") return <Badge tone="red">خطا در اتصال</Badge>;
  if (status === "UNTESTED") return <Badge tone="amber">تست‌نشده</Badge>;
  return <Badge tone="gray">بدون اتصال</Badge>;
}
