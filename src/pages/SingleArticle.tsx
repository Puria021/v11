import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, CampaignView, TitleOption } from "../api/tauri";
import { Button, Card, ErrorBox, inputClass } from "../components/ui";
import StepGenerate from "../components/StepGenerate";
import StepReview from "../components/StepReview";
import { errorMessage, num } from "../lib/format";

const STEP_LABELS = ["ورودی مقاله", "انتخاب عنوان", "تولید مقاله", "ویرایش و آپلود"];

export default function SingleArticle() {
  const qc = useQueryClient();
  const [step, setStep] = useState(1);
  const [campaignId, setCampaignId] = useState<string | null>(null);
  const [planId, setPlanId] = useState<string | null>(null);

  const quota = useQuery({ queryKey: ["quota"], queryFn: api.getQuota });
  const campaign = useQuery({
    queryKey: ["campaign", campaignId],
    queryFn: () => api.getCampaign(campaignId!),
    enabled: !!campaignId,
  });
  const refreshCampaign = () => {
    if (campaignId) qc.invalidateQueries({ queryKey: ["campaign", campaignId] });
    qc.invalidateQueries({ queryKey: ["quota"] });
  };

  const c: CampaignView | undefined = campaign.data;
  const articlesLeft = quota.data?.articles;

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold">تولید مقاله‌ی تکی</h1>
          <p className="mt-1 text-sm text-slate-500">یک مقاله با تنظیمات دلخواه بنویسید؛ بدون ساخت کمپین.</p>
        </div>
        {articlesLeft && (
          <div className="rounded-xl border border-slate-200 bg-white px-4 py-2 text-sm dark:border-slate-700 dark:bg-slate-800">
            مقاله‌های باقی‌مانده: <span className="font-bold text-brand-700">{num(articlesLeft.remaining)}</span> از {num(articlesLeft.total)}
          </div>
        )}
      </div>

      <ol className="flex flex-wrap gap-2">
        {STEP_LABELS.map((label, i) => {
          const n = i + 1;
          const reachable = n === 1 || (!!campaignId && n <= step + 1);
          return (
            <li key={label}>
              <button
                disabled={!reachable}
                onClick={() => setStep(n)}
                className={`rounded-full px-4 py-1.5 text-sm disabled:opacity-40 ${step === n ? "bg-brand-600 text-white" : "bg-white text-slate-600 ring-1 ring-slate-200 hover:bg-slate-50 dark:bg-slate-800 dark:text-slate-300 dark:ring-slate-700"}`}
              >
                {num(n)}. {label}
              </button>
            </li>
          );
        })}
      </ol>

      {step === 1 && (
        <SingleForm
          disabled={(articlesLeft?.available ?? 1) < 1}
          onCreated={(cid, pid) => {
            setCampaignId(cid);
            setPlanId(pid);
            setStep(2);
            refreshCampaign();
          }}
        />
      )}
      {step === 2 && campaignId && planId && (
        <TitlePicker
          campaignId={campaignId}
          planId={planId}
          onApproved={() => {
            refreshCampaign();
            setStep(3);
          }}
        />
      )}
      {step >= 3 && c && (
        <>
          {step === 3 && <StepGenerate c={c} onChanged={refreshCampaign} next={() => setStep(4)} />}
          {step === 4 && (
            <>
              <Button variant="secondary" onClick={() => setStep(3)}>← بازگشت به تولید</Button>
              <StepReview c={c} onChanged={refreshCampaign} />
            </>
          )}
        </>
      )}
      {step >= 3 && !c && <p className="text-sm text-slate-500">در حال بارگذاری…</p>}
    </div>
  );
}

function SingleForm({ disabled, onCreated }: { disabled: boolean; onCreated: (campaignId: string, planId: string) => void }) {
  const projects = useQuery({ queryKey: ["projects"], queryFn: api.listProjects });
  const [projectId, setProjectId] = useState("");
  const [keyword, setKeyword] = useState("");
  const [imageCount, setImageCount] = useState(1);
  const [externalUrl, setExternalUrl] = useState("");
  const [externalTitle, setExternalTitle] = useState("");
  const [follow, setFollow] = useState(true);
  const [cta, setCta] = useState("");
  const [error, setError] = useState<string | null>(null);

  const selectedProject = projectId || projects.data?.[0]?.id || "";
  const create = useMutation({
    mutationFn: () =>
      api.createSingleArticle({
        projectId: selectedProject,
        keyword: keyword.trim(),
        imageCount,
        externalUrl: externalUrl.trim() || null,
        externalTitle: externalTitle.trim() || null,
        externalFollow: follow,
        cta: cta.trim() || null,
      }),
    onSuccess: (r) => { setError(null); onCreated(r.campaignId, r.planId); },
    onError: (e) => setError(errorMessage(e)),
  });

  return (
    <Card className="max-w-3xl space-y-5">
      {projects.data && projects.data.length === 0 && <ErrorBox message="ابتدا یک پروژه بسازید." />}
      <div className="grid gap-4 md:grid-cols-2">
        <label className="space-y-1 md:col-span-2">
          <span className="text-sm font-medium">کلمه‌ی کلیدی</span>
          <input className={inputClass} value={keyword} onChange={(e) => setKeyword(e.target.value)} placeholder="مثلاً: خرید لپ تاپ دانشجویی" />
        </label>
        <label className="space-y-1">
          <span className="text-sm font-medium">پروژه</span>
          <select className={inputClass} value={selectedProject} onChange={(e) => setProjectId(e.target.value)}>
            {(projects.data ?? []).map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
          </select>
        </label>
        <label className="space-y-1">
          <span className="text-sm font-medium">تعداد تصاویر</span>
          <select className={inputClass} value={imageCount} onChange={(e) => setImageCount(Number(e.target.value))}>
            {[1, 2, 3, 4].map((n) => <option key={n} value={n}>{num(n)}</option>)}
          </select>
        </label>
        <label className="space-y-1">
          <span className="text-sm font-medium">لینک خارجی (اختیاری)</span>
          <input className={`${inputClass} ltr`} value={externalUrl} onChange={(e) => setExternalUrl(e.target.value)} placeholder="https://example.org/..." />
          <span className="block text-xs text-slate-500">خالی بماند؛ ارائه‌دهنده یک منبع معتبر انتخاب می‌کند.</span>
        </label>
        <label className="space-y-1">
          <span className="text-sm font-medium">عنوان لینک خارجی (اختیاری)</span>
          <input className={inputClass} value={externalTitle} onChange={(e) => setExternalTitle(e.target.value)} placeholder="متن لنگر لینک" />
        </label>
        <div className="space-y-1">
          <span className="text-sm font-medium">نوع لینک خارجی</span>
          <div className="flex gap-2">
            <button type="button" onClick={() => setFollow(true)} className={`rounded-lg px-3 py-2 text-sm ${follow ? "bg-brand-600 text-white" : "bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300"}`}>فالو</button>
            <button type="button" onClick={() => setFollow(false)} className={`rounded-lg px-3 py-2 text-sm ${!follow ? "bg-brand-600 text-white" : "bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300"}`}>نوفالو</button>
          </div>
        </div>
        <label className="space-y-1 md:col-span-2">
          <span className="text-sm font-medium">CTA اختصاصی مقاله (اختیاری)</span>
          <textarea rows={2} className={inputClass} value={cta} onChange={(e) => setCta(e.target.value)} placeholder="خالی بماند تا ارائه‌دهنده خودش بنویسد" />
        </label>
      </div>
      <ErrorBox message={error} />
      <Button disabled={disabled || !keyword.trim() || !selectedProject || create.isPending} onClick={() => { setError(null); create.mutate(); }}>
        {create.isPending ? "در حال ساخت…" : "ادامه: پیشنهاد عنوان"}
      </Button>
      {disabled && <p className="text-xs text-red-600">سهمیه‌ی مقاله‌های این لایسنس تمام شده است.</p>}
    </Card>
  );
}

function TitlePicker({ campaignId, planId, onApproved }: { campaignId: string; planId: string; onApproved: () => void }) {
  const [options, setOptions] = useState<TitleOption[]>([]);
  const [chosen, setChosen] = useState<string | null>(null);
  const [manual, setManual] = useState("");
  const [error, setError] = useState<string | null>(null);

  const suggest = useMutation({
    mutationFn: () => api.suggestSingleTitles(campaignId),
    onSuccess: (list) => { setOptions(list); setChosen(list[0]?.id ?? null); setError(null); },
    onError: (e) => setError(errorMessage(e)),
  });
  // Ask for the first three titles once when this step opens.
  const asked = useRef(false);
  useEffect(() => {
    if (asked.current) return;
    asked.current = true;
    suggest.mutate();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const approve = useMutation({
    mutationFn: async () => {
      if (manual.trim()) {
        await api.editTitle(planId, manual.trim());
      } else if (chosen) {
        await api.approveTitle(planId, chosen);
      } else {
        throw new Error("یک عنوان انتخاب یا وارد کنید.");
      }
    },
    onSuccess: () => { setError(null); onApproved(); },
    onError: (e) => setError(errorMessage(e)),
  });

  return (
    <Card className="max-w-3xl space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="font-semibold">عنوان مقاله را انتخاب کنید</div>
        <Button variant="secondary" disabled={suggest.isPending} onClick={() => suggest.mutate()}>
          {suggest.isPending ? "در حال پیشنهاد…" : "پیشنهادهای جدید"}
        </Button>
      </div>
      <div className="space-y-2">
        {options.map((o) => (
          <label key={o.id} className={`flex cursor-pointer items-start gap-3 rounded-lg border p-3 ${chosen === o.id && !manual.trim() ? "border-brand-500 bg-brand-50 dark:bg-slate-800" : "border-slate-200 dark:border-slate-700"}`}>
            <input type="radio" className="mt-1" name="title" checked={chosen === o.id && !manual.trim()} onChange={() => { setChosen(o.id); setManual(""); }} />
            <span>{o.title}</span>
          </label>
        ))}
        {suggest.isPending && options.length === 0 && <p className="text-sm text-slate-500">در حال پیشنهاد عنوان…</p>}
      </div>
      <label className="block space-y-1">
        <span className="text-sm font-medium">یا عنوان دلخواه</span>
        <input className={inputClass} value={manual} onChange={(e) => setManual(e.target.value)} placeholder="عنوان خودتان" />
      </label>
      <ErrorBox message={error} />
      <Button disabled={approve.isPending || (!manual.trim() && !chosen)} onClick={() => approve.mutate()}>
        {approve.isPending ? "در حال ثبت…" : "تأیید عنوان و شروع تولید ←"}
      </Button>
    </Card>
  );
}
