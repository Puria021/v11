import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ArticleDetail, ArticleRow, CampaignView } from "../api/tauri";
import { Badge, Button, Card, ConfirmDialog, ErrorBox, Modal } from "./ui";
import { errorMessage, num } from "../lib/format";
import ArticlePreviewBody from "./ArticlePreviewBody";

export const STATUS: Record<string, [string, "gray" | "amber" | "green" | "red"]> = {
  PLANNED: ["برنامه‌ریزی", "gray"], TITLE_APPROVED: ["عنوان تأییدشده", "gray"], QUEUED: ["در صف", "amber"],
  GENERATING: ["در حال نوشتن…", "amber"], CONTENT_GENERATED: ["متن آماده", "amber"], IMAGE_GENERATING: ["در حال ساخت تصویر…", "amber"],
  READY_TO_UPLOAD: ["آماده‌ی آپلود", "green"], UPLOADING: ["در حال آپلود", "amber"], DRAFT_CREATED: ["پیش‌نویس ساخته شد", "green"],
  FAILED: ["ناموفق", "red"], CANCELLED: ["لغو شده", "gray"],
};

export default function StepGenerate({ c, onChanged, next }: { c: CampaignView; onChanged: () => void; next: () => void }) {
  const qc = useQueryClient();
  const progress = useQuery({
    queryKey: ["progress", c.id],
    queryFn: () => api.getProgress(c.id),
    refetchInterval: (q) => (q.state.data?.campaignStatus === "IN_PROGRESS" ? 3000 : false),
  });
  const articles = useQuery({
    queryKey: ["articles", c.id],
    queryFn: () => api.listArticles(c.id),
    refetchInterval: progress.data?.campaignStatus === "IN_PROGRESS" ? 3000 : false,
  });
  const [error, setError] = useState<string | null>(null);
  const [confirmCancel, setConfirmCancel] = useState(false);
  const [viewing, setViewing] = useState<string | null>(null);

  // Live updates pushed from the Rust workers.
  useEffect(() => {
    const refresh = () => {
      qc.invalidateQueries({ queryKey: ["progress", c.id] });
      qc.invalidateQueries({ queryKey: ["articles", c.id] });
    };
    const subs = [listen("campaign:progress", refresh), listen("plan:status", refresh), listen("campaign:status", () => { refresh(); onChanged(); })];
    return () => { subs.forEach((p) => p.then((f) => f())); };
  }, [c.id, qc, onChanged]);

  const after = () => {
    qc.invalidateQueries({ queryKey: ["progress", c.id] });
    qc.invalidateQueries({ queryKey: ["articles", c.id] });
    onChanged();
  };
  const fail = (e: unknown) => setError(errorMessage(e));
  const run = (fn: () => Promise<unknown>) => ({ mutationFn: fn, onSuccess: () => { setError(null); after(); }, onError: fail });

  const start = useMutation(run(() => api.startGeneration(c.id)));
  const pause = useMutation(run(() => api.pauseCampaign(c.id)));
  const resume = useMutation(run(() => api.resumeCampaign(c.id)));
  const cancel = useMutation({ ...run(() => api.cancelCampaign(c.id)), onSuccess: () => { setConfirmCancel(false); after(); } });
  const retryFailed = useMutation(run(() => api.retryFailed(c.id)));

  const p = progress.data;
  const status = p?.campaignStatus ?? c.status;
  // Template and image provider are fixed; the backend fills them in on start.
  const ready = !!c.aiProviderId;
  const started = status === "IN_PROGRESS" || status === "PAUSED" || (p?.content.done ?? 0) > 0 || (p?.queued ?? 0) > 0;
  const allApproved = c.approvedCount === c.planCount && c.planCount > 0;

  // Generation starts by itself once every title is approved; it is not a separate user step.
  const autoStarted = useRef(false);
  useEffect(() => {
    if (autoStarted.current || !progress.isSuccess || !progress.data || !allApproved || !ready) return;
    const d = progress.data;
    if (d.campaignStatus !== "DRAFT" || d.content.done > 0 || d.queued > 0) return;
    autoStarted.current = true;
    start.mutate();
  }, [progress.isSuccess, progress.data, allApproved, ready]);

  if (!allApproved) {
    return <Card><p className="text-sm text-slate-600">ابتدا همه‌ی {num(c.planCount)} عنوان را در مرحله‌ی قبل تأیید کنید.</p></Card>;
  }

  return (
    <div className="space-y-4">
      <Card className="space-y-4">
        {!ready && <p className="text-xs text-amber-700">لایسنس را وارد کنید.</p>}
        <ErrorBox message={error} />

        <div className="flex flex-wrap gap-2">
          {status === "CANCELLED" && (
            <Button disabled={!ready || start.isPending} onClick={() => start.mutate()}>ادامه‌ی تولید</Button>
          )}
          {status === "IN_PROGRESS" && <Button variant="secondary" disabled={pause.isPending} onClick={() => pause.mutate()}>توقف موقت</Button>}
          {status === "PAUSED" && <Button disabled={resume.isPending} onClick={() => resume.mutate()}>ادامه</Button>}
          {(status === "IN_PROGRESS" || status === "PAUSED") && <Button variant="danger" onClick={() => setConfirmCancel(true)}>لغو</Button>}
          {(p?.complete ?? 0) > 0 && <Button variant="secondary" onClick={next}>ادامه: ویرایش و آپلود ←</Button>}
          {(p?.failed ?? 0) > 0 && <Button variant="secondary" disabled={retryFailed.isPending} onClick={() => retryFailed.mutate()}>تلاش مجدد برای همه ({num(p!.failed)} مورد)</Button>}
        </div>
        {status === "PAUSED" && <p className="text-xs text-amber-700">متوقف شد؛ کارهای در حال اجرا تمام می‌شوند ولی کار جدیدی شروع نمی‌شود.</p>}
      </Card>

      {p && started && (
        <Card className="space-y-3">
          <div className="flex items-center justify-between text-sm">
            <span className="font-semibold">پیشرفت تولید</span>
            <span>{num(p.percent)}٪</span>
          </div>
          <div className="h-3 overflow-hidden rounded-full bg-slate-100">
            <div className="h-full rounded-full bg-brand-600 transition-all" style={{ width: `${p.percent}%` }} />
          </div>
          <div className="grid grid-cols-2 gap-3 text-center text-sm md:grid-cols-5">
            <Stat label="عنوان‌ها" v={`${num(p.titles.done)}/${num(p.titles.total)}`} />
            <Stat label="کامل (متن + تصویر)" v={`${num(p.complete)}/${num(p.titles.total)}`} />
            <Stat label="تصویر" v={p.imagesEnabled ? `${num(p.images.done)}/${num(p.images.total)}` : "—"} />
            <Stat label="وردپرس" v={`${num(p.wordpress.done)}/${num(p.wordpress.total)}`} />
            <Stat label="ناموفق" v={num(p.failed)} tone={p.failed > 0 ? "red" : undefined} />
          </div>
          <div className="text-xs text-slate-500">در حال اجرا: {num(p.running)} · در صف: {num(p.queued)}</div>
        </Card>
      )}

      {started && articles.data && (
        <div className="overflow-auto rounded-xl border border-slate-200 bg-white">
          <table className="w-full text-sm">
            <thead className="bg-slate-50 text-slate-600">
              <tr><th className="w-14 px-3 py-2 text-start">#</th><th className="px-3 py-2 text-start">عنوان</th><th className="px-3 py-2 text-start">وضعیت</th><th className="px-3 py-2 text-start">کلمات</th><th className="px-3 py-2 text-start">تصویر</th><th className="px-3 py-2 text-start">عملیات</th></tr>
            </thead>
            <tbody>
              {articles.data.map((a) => <Row key={a.planId} a={a} onView={() => setViewing(a.planId)} onChanged={after} onError={fail} />)}
            </tbody>
          </table>
        </div>
      )}

      {confirmCancel && (
        <ConfirmDialog title="لغو تولید" message="کارهای در صف لغو می‌شوند و مقاله‌هایی که در حال نوشتن‌اند دور ریخته می‌شوند. مقاله‌های کامل‌شده می‌مانند و بعداً می‌توانید ادامه دهید."
          confirmLabel="لغو تولید" busy={cancel.isPending} onConfirm={() => cancel.mutate()} onCancel={() => setConfirmCancel(false)} />
      )}
      {viewing && <ArticleViewer planId={viewing} onClose={() => setViewing(null)} />}
    </div>
  );
}

function Stat({ label, v, tone }: { label: string; v: string; tone?: "red" }) {
  return (
    <div className="rounded-lg bg-slate-50 p-2">
      <div className={`text-lg font-bold ${tone === "red" ? "text-red-600" : "text-brand-700"}`}>{v}</div>
      <div className="text-xs text-slate-500">{label}</div>
    </div>
  );
}

function Row({ a, onView, onChanged, onError }: { a: ArticleRow; onView: () => void; onChanged: () => void; onError: (e: unknown) => void }) {
  const [label, tone] = STATUS[a.status] ?? [a.status, "gray" as const];
  const retry = useMutation({
    mutationFn: (stage: "content" | "image") => api.retryArticle(a.planId, stage),
    onSuccess: onChanged,
    onError,
  });
  const hasContent = a.wordCount !== null;
  const busy = ["QUEUED", "GENERATING", "IMAGE_GENERATING"].includes(a.status);
  return (
    <tr className="border-t border-slate-100 align-top">
      <td className="px-3 py-2 text-slate-400">{num(a.articleNo)}</td>
      <td className="px-3 py-2">
        <div>{a.title ?? a.keyword}</div>
        {a.status === "FAILED" && a.lastError && <div className="mt-1 text-xs text-red-600">{a.lastError}</div>}
        {a.status !== "FAILED" && a.lastError && busy && <div className="mt-1 text-xs text-amber-700">تلاش مجدد: {a.lastError}</div>}
      </td>
      <td className="px-3 py-2"><Badge tone={tone}>{label}</Badge></td>
      <td className="px-3 py-2">{a.wordCount !== null ? num(a.wordCount) : "—"}</td>
      <td className="px-3 py-2">{a.hasImage ? "✓" : "—"}</td>
      <td className="whitespace-nowrap px-3 py-2">
        <div className="flex gap-1">
          <Button variant="ghost" disabled={!hasContent} onClick={onView}>مشاهده</Button>
          {!busy && <Button variant="ghost" disabled={retry.isPending} onClick={() => retry.mutate(a.failedStage === "image" ? "image" : "content")}>{hasContent && a.status !== "FAILED" ? "تولید دوباره" : "تلاش مجدد"}</Button>}
          {!busy && hasContent && a.status !== "FAILED" && <Button variant="ghost" disabled={retry.isPending} onClick={() => retry.mutate("image")}>تصویر دوباره</Button>}
        </div>
      </td>
    </tr>
  );
}

function ArticleViewer({ planId, onClose }: { planId: string; onClose: () => void }) {
  const art = useQuery({ queryKey: ["article", planId], queryFn: () => api.getArticle(planId) });
  const a: ArticleDetail | undefined = art.data;
  return (
    <Modal title={a?.title ?? "مقاله"} onClose={onClose} wide>
      {art.error && <ErrorBox message={errorMessage(art.error)} />}
      {a && (
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-2 text-xs text-slate-600">
            <div>کلمات: {num(a.wordCount)}</div>
            <div>Slug: <span className="ltr inline-block">{a.slug}</span></div>
            <div className="col-span-2">Meta Title: {a.metaTitle}</div>
            <div className="col-span-2">Meta Description: {a.metaDescription}</div>
          </div>
          {(a.validation?.warnings?.length ?? 0) > 0 && (
            <div className="rounded-lg border border-amber-200 bg-amber-50 p-2 text-xs text-amber-900">
              {a.validation!.warnings!.map((w) => <div key={w}>⚠ {w}</div>)}
            </div>
          )}
          <ArticlePreviewBody planId={planId} html={a.contentHtml} />
          {a.faq.length > 0 && (
            <div>
              <h3 className="mb-2 font-bold">پرسش‌های متداول</h3>
              {a.faq.map((f) => <div key={f.q} className="mb-2 text-sm"><div className="font-semibold">{f.q}</div><div>{f.a}</div></div>)}
            </div>
          )}
        </div>
      )}
    </Modal>
  );
}
