import { useEffect, useRef, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, CampaignView, Intent, Keyword, PlanView } from "../api/tauri";
import { Badge, Button, Card, ConfirmDialog, ErrorBox, inputClass } from "../components/ui";
import { errorMessage, num } from "../lib/format";
import StepGenerate from "../components/StepGenerate";
import StepReview from "../components/StepReview";

const STEPS = [
  { id: 1, label: "کلمات اصلی", wizard: 2 },
  { id: 2, label: "تحقیق و انتخاب کلمات", wizard: 4 },
  { id: 3, label: "کلمات ثانویه", wizard: 6 },
  { id: 4, label: "عنوان‌ها", wizard: 8 },
  { id: 5, label: "تولید مقاله", wizard: 9 },
  { id: 6, label: "ویرایش و آپلود", wizard: 12 },
];
const stepFromWizard = (w: number) => (w >= 12 ? 6 : w >= 9 ? 5 : w >= 7 ? 4 : w >= 5 ? 3 : w >= 3 ? 2 : 1);

const INTENT_FA: Record<Intent, string> = {
  informational: "اطلاعاتی", commercial: "تجاری", transactional: "خرید", navigational: "ناوبری",
};
const SOURCE_FA = { keyword_api: "API کلمات کلیدی", ai: "پیشنهاد AI", user: "ورودی کاربر" } as const;

export default function CampaignWizard({ online }: { online: boolean | null }) {
  const { id = "" } = useParams();
  const qc = useQueryClient();
  const campaign = useQuery({ queryKey: ["campaign", id], queryFn: () => api.getCampaign(id) });
  const [step, setStep] = useState<number | null>(null);

  useEffect(() => {
    if (campaign.data && step === null) setStep(stepFromWizard(campaign.data.wizardStep));
  }, [campaign.data, step]);

  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["campaign", id] });
    qc.invalidateQueries({ queryKey: ["campaigns"] });
    qc.invalidateQueries({ queryKey: ["stats"] });
  };
  const goto = (s: number) => {
    setStep(s);
    api.saveWizardState(id, STEPS[s - 1].wizard, { step: s }).catch(() => undefined);
  };

  if (campaign.isLoading || step === null) return <p className="text-sm text-slate-500">در حال بارگذاری…</p>;
  if (campaign.error || !campaign.data) return <ErrorBox message={errorMessage(campaign.error ?? "کمپین پیدا نشد.")} />;
  const c = campaign.data;
  const locked = c.planCount > 0;

  return (
    <div className="space-y-6">
      <div>
        <Link to="/campaigns" className="text-sm text-brand-600">→ بازگشت به کمپین‌ها</Link>
        <h1 className="mt-2 text-2xl font-bold">{c.name}</h1>
        <div className="text-sm text-slate-500">{c.projectName}</div>
      </div>

      <ol className="flex flex-wrap gap-2">
        {STEPS.map((s) => (
          <li key={s.id}>
            <button
              onClick={() => goto(s.id)}
              className={`rounded-full px-4 py-1.5 text-sm ${step === s.id ? "bg-brand-600 text-white" : "bg-white text-slate-600 ring-1 ring-slate-200 hover:bg-slate-50"}`}
            >
              {num(s.id)}. {s.label}
            </button>
          </li>
        ))}
      </ol>

      {online === false && (
        <div className="rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-800">
          به نظر می‌رسد اینترنت قطع است. عملیات AI ممکن است با خطا مواجه شوند؛ کارهای محلی (CSV، ویرایش) بدون اینترنت کار می‌کنند.
        </div>
      )}

      {step === 1 && <StepSeeds c={c} onChanged={refresh} next={() => goto(2)} />}
      {step === 2 && <StepKeywords c={c} onChanged={refresh} next={() => goto(3)} />}
      {step === 3 && <StepSecondary c={c} locked={locked} onChanged={refresh} next={() => goto(4)} />}
      {step === 4 && <StepTitles c={c} onChanged={refresh} next={() => goto(5)} />}
      {(step === 5 || step === 6) && <Button variant="secondary" onClick={() => { refresh(); qc.refetchQueries({ type: "active" }); }}>↻ رفرش لحظه‌ای مرحله {step}</Button>}
      {step === 5 && <StepGenerate c={c} onChanged={refresh} next={() => goto(6)} />}
      {step === 6 && <StepReview c={c} onChanged={refresh} />}
    </div>
  );
}

/* ------------------------------------------------------------------ step 1 */
function StepSeeds({ c, onChanged, next }: { c: CampaignView; onChanged: () => void; next: () => void }) {
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const loaded = useRef(false);

  const existing = useQuery({
    queryKey: ["primary", c.id],
    queryFn: () => api.listKeywords({ campaignId: c.id, role: "primary", pageSize: 100 }),
  });
  useEffect(() => {
    if (existing.data && !loaded.current) {
      loaded.current = true;
      setText(existing.data.items.map((k) => k.keyword).join("\n"));
    }
  }, [existing.data]);

  const save = useMutation({
    mutationFn: () => api.savePrimaryKeywords(c.id, text.split("\n")),
    onSuccess: () => { onChanged(); next(); },
    onError: (e) => setError(errorMessage(e)),
  });

  return (
    <Card className="max-w-2xl space-y-4">
      <div>
        <label className="mb-1 block text-sm font-medium">کلمات کلیدی اصلی (هر خط یک کلمه)</label>
        <textarea className={inputClass} rows={6} value={text} onChange={(e) => setText(e.target.value)} placeholder={"خرید لپ تاپ\nبهترین لپ تاپ\nلپ تاپ دانشجویی"} />
      </div>
      <ErrorBox message={error} />
      <Button disabled={!text.trim() || save.isPending} onClick={() => { setError(null); save.mutate(); }}>
        {save.isPending ? "در حال ذخیره…" : "ذخیره و ادامه"}
      </Button>
    </Card>
  );
}

/* ------------------------------------------------------------------ step 2 */
function StepKeywords({ c, onChanged, next }: { c: CampaignView; onChanged: () => void; next: () => void }) {
  const qc = useQueryClient();
  const [search, setSearch] = useState("");
  const [sortBy, setSortBy] = useState("keyword");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("asc");
  const [source, setSource] = useState("");
  const [manual, setManual] = useState("");
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const key = ["keywords", c.id, search, sortBy, sortDir, source];
  const list = useQuery({
    queryKey: key,
    queryFn: () => api.listKeywords({ campaignId: c.id, role: "candidate", search, source: source || undefined, sortBy, sortDir, pageSize: 500 }),
  });
  const after = () => { qc.invalidateQueries({ queryKey: ["keywords", c.id] }); onChanged(); };
  const fail = (e: unknown) => { setMsg(null); setError(errorMessage(e)); };

  const importCsv = useMutation({
    mutationFn: async (file: File) => api.importKeywordsCsv(c.id, await file.text()),
    onSuccess: (r) => { setError(null); setMsg(`${num(r.imported)} کلمه‌ی جدید، ${num(r.updated)} به‌روزرسانی، ${num(r.skipped)} رد شد.`); after(); },
    onError: fail,
  });
  const suggest = useMutation({
    mutationFn: () => api.aiSuggestKeywords(c.id, 30),
    onSuccess: (r) => { setError(null); setMsg(`${num(r.added)} پیشنهاد AI اضافه شد (بدون داده‌ی جستجو).`); after(); },
    onError: fail,
  });
  const addManual = useMutation({
    mutationFn: () => api.addManualKeyword(c.id, manual),
    onSuccess: () => { setManual(""); setError(null); after(); },
    onError: fail,
  });
  const select = useMutation({
    mutationFn: ({ ids, sel }: { ids: string[]; sel: boolean }) => api.selectKeywords(c.id, ids, sel),
    onSuccess: after,
    onError: fail,
  });
  const remove = useMutation({
    mutationFn: (ids: string[]) => api.deleteKeywords(c.id, ids),
    onSuccess: after,
    onError: fail,
  });

  const items = list.data?.items ?? [];
  const allSelected = items.length > 0 && items.every((k) => k.isSelected);
  const toggleSort = (col: string) => {
    if (sortBy === col) setSortDir(sortDir === "asc" ? "desc" : "asc");
    else { setSortBy(col); setSortDir("desc"); }
  };
  const th = (col: string, label: string) => (
    <th className="cursor-pointer px-3 py-2 text-start font-medium" onClick={() => toggleSort(col)}>
      {label}{sortBy === col ? (sortDir === "asc" ? " ▲" : " ▼") : ""}
    </th>
  );

  return (
    <div className="space-y-4">
      <Card className="space-y-3">
        <div className="flex flex-wrap items-center gap-2">
          <input ref={fileRef} type="file" accept=".csv,.txt,text/csv" className="hidden"
            onChange={(e) => { const f = e.target.files?.[0]; if (f) importCsv.mutate(f); e.target.value = ""; }} />
          <Button variant="secondary" disabled={importCsv.isPending} onClick={() => fileRef.current?.click()}>ورود CSV</Button>
          <Button variant="secondary" className="ai-btn" disabled={suggest.isPending || !c.aiProviderId} onClick={() => { setMsg(null); suggest.mutate(); }}>
            {suggest.isPending ? "در حال پرسش از AI…" : "پیشنهاد کلمه با AI"}
          </Button>
          <div className="flex gap-1">
            <input className={`${inputClass} w-56`} placeholder="افزودن دستی…" value={manual} onChange={(e) => setManual(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && manual.trim() && addManual.mutate()} />
            <Button variant="secondary" disabled={!manual.trim() || addManual.isPending} onClick={() => addManual.mutate()}>افزودن</Button>
          </div>
        </div>
        {!c.aiProviderId && <p className="text-xs text-amber-700">ارائه‌دهنده‌ی AI فعال نیست؛ ابتدا لایسنس را فعال کنید.</p>}
        {msg && <p className="text-sm text-emerald-700">{msg}</p>}
        <ErrorBox message={error} />
      </Card>

      <div className="flex flex-wrap items-center gap-3">
        <input className={`${inputClass} w-64`} placeholder="جستجو در کلمات…" value={search} onChange={(e) => setSearch(e.target.value)} />
        <select className={`${inputClass} w-44`} value={source} onChange={(e) => setSource(e.target.value)}>
          <option value="">همه‌ی منابع</option>
          <option value="user">ورودی کاربر</option>
          <option value="keyword_api">API کلمات کلیدی</option>
          <option value="ai">پیشنهاد AI</option>
        </select>
        <span className="text-sm text-slate-500">{num(list.data?.total ?? 0)} کلمه · {num(c.selectedCount)} انتخاب‌شده</span>
      </div>

      <div className="overflow-auto rounded-xl border border-slate-200 bg-white">
        <table className="w-full text-sm">
          <thead className="bg-slate-50 text-slate-600">
            <tr>
              <th className="w-10 px-3 py-2">
                <input type="checkbox" checked={allSelected} onChange={() => select.mutate({ ids: items.map((k) => k.id), sel: !allSelected })} />
              </th>
              {th("keyword", "کلمه‌ی کلیدی")}
              {th("intent", "هدف جستجو")}
              <th className="px-3 py-2 text-start font-medium">منبع</th>
              <th className="w-10" />
            </tr>
          </thead>
          <tbody>
            {items.map((k) => <KeywordRow key={k.id} k={k} onToggle={() => select.mutate({ ids: [k.id], sel: !k.isSelected })} onDelete={() => remove.mutate([k.id])} />)}
            {items.length === 0 && (
              <tr><td colSpan={6} className="px-3 py-8 text-center text-slate-500">کلمه‌ای وجود ندارد. CSV وارد کنید، از AI پیشنهاد بگیرید یا دستی اضافه کنید.</td></tr>
            )}
          </tbody>
        </table>
      </div>

      <Button disabled={c.selectedCount === 0 && c.primaryCount === 0} onClick={next}>ادامه: کلمات ثانویه</Button>
    </div>
  );
}

function KeywordRow({ k, onToggle, onDelete }: { k: Keyword; onToggle: () => void; onDelete: () => void }) {
  return (
    <tr className="border-t border-slate-100 hover:bg-slate-50">
      <td className="px-3 py-2"><input type="checkbox" checked={k.isSelected} onChange={onToggle} /></td>
      <td className="px-3 py-2">{k.keyword}{k.related.length > 0 && <span className="ms-2 text-xs text-slate-400">+{num(k.related.length)} مرتبط</span>}</td>
      <td className="px-3 py-2 text-xs">{k.intent ? INTENT_FA[k.intent] : "—"}</td>
      <td className="px-3 py-2"><Badge tone={k.source === "ai" ? "amber" : "gray"}>{SOURCE_FA[k.source]}</Badge></td>
      <td className="px-2"><button className="text-slate-400 hover:text-red-600" title="حذف" onClick={onDelete}>✕</button></td>
    </tr>
  );
}

/* ------------------------------------------------------------------ step 3 */
function StepSecondary({ c, locked, onChanged, next }: { c: CampaignView; locked: boolean; onChanged: () => void; next: () => void }) {
  const qc = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [confirmReset, setConfirmReset] = useState(false);
  const [manualSecondary, setManualSecondary] = useState("");
  const [editingSecondary, setEditingSecondary] = useState<{ id: string; value: string } | null>(null);

  const list = useQuery({ queryKey: ["secondary", c.id, c.secondaryCount], queryFn: () => api.listKeywords({ campaignId: c.id, role: "secondary", pageSize: 500 }) });
  const warnings = useQuery({ queryKey: ["simwarn", c.id, c.secondaryCount], queryFn: () => api.getSimilarityWarnings(c.id) });
  const after = () => { qc.invalidateQueries({ queryKey: ["secondary", c.id] }); qc.invalidateQueries({ queryKey: ["simwarn", c.id] }); onChanged(); };
  const fail = (e: unknown) => { setMsg(null); setError(errorMessage(e)); };

  const generate = useMutation({
    mutationFn: (replace: boolean) => api.generateSecondaryKeywords(c.id, replace),
    onSuccess: (r) => {
      setError(null);
      setMsg(r.missing === 0 ? `✓ ${num(r.total)} کلمه‌ی ثانویه آماده است.` : `فقط ${num(r.total)} از ${num(r.target)} کلمه پیدا شد (${num(r.missing)} کم است). دوباره «تکمیل کمبود» را بزنید یا دستی اضافه کنید. چیزی ساختگی اضافه نشد.`);
      after();
    },
    onError: fail,
  });
  const remove = useMutation({ mutationFn: (id: string) => api.deleteKeywords(c.id, [id]), onSuccess: after, onError: fail });
  const addSecondary = useMutation({ mutationFn: () => api.addSecondaryKeyword(c.id, manualSecondary), onSuccess: () => {setManualSecondary("");setError(null);after();}, onError: fail });
  const editSecondary = useMutation({ mutationFn: () => api.editCampaignKeyword(c.id, editingSecondary!.id, editingSecondary!.value), onSuccess: () => {setEditingSecondary(null);setError(null);after();}, onError: fail });
  const confirm = useMutation({ mutationFn: () => api.confirmSecondaryKeywords(c.id), onSuccess: () => { setError(null); onChanged(); next(); }, onError: fail });
  const reset = useMutation({ mutationFn: () => api.resetArticlePlans(c.id), onSuccess: () => { setConfirmReset(false); after(); }, onError: (e) => { setConfirmReset(false); fail(e); } });

  const items = list.data?.items ?? [];
  const flagged = new Set((warnings.data ?? []).flatMap((w) => [w.aId, w.bId]));
  const ready = c.secondaryCount === c.targetCount;

  return (
    <div className="space-y-4">
      <Card className="space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <div className="text-lg font-semibold">{num(c.secondaryCount)} / {num(c.targetCount)} کلمه‌ی ثانویه</div>
            <div className="text-xs text-slate-500">پیشنهادها بر اساس {num(c.selectedCount)} کلمه‌ی انتخاب‌شده در مرحله‌ی ۲ و کلمات اصلی ساخته می‌شود.</div>
          </div>
          {!locked && (
            <div className="flex flex-wrap gap-2">
              <Button className="ai-btn" disabled={generate.isPending || !c.aiProviderId} onClick={() => { setMsg(null); setError(null); generate.mutate(c.secondaryCount > 0 && !ready ? false : true); }}>
                {generate.isPending ? "در حال تولید (ممکن است چند دقیقه طول بکشد)…" : c.secondaryCount === 0 ? `تولید ${num(c.targetCount)} کلمه‌ی ثانویه` : ready ? "تولید دوباره (جایگزینی همه)" : "تکمیل کمبود"}
              </Button>
              {c.secondaryCount > 0 && !ready && (
                <Button variant="secondary" disabled={generate.isPending} onClick={() => generate.mutate(true)}>تولید دوباره (جایگزینی همه)</Button>
              )}
            </div>
          )}
        </div>
        {!locked && <div className="flex flex-wrap gap-2 items-center"><input className={`${inputClass} flex-1`} placeholder="افزودن کلمه کلیدی ثانویه به صورت دستی" value={manualSecondary} onChange={e=>setManualSecondary(e.target.value)} onKeyDown={e=>{if(e.key==='Enter' && manualSecondary.trim() && !addSecondary.isPending) addSecondary.mutate();}} /><Button variant="secondary" disabled={addSecondary.isPending || !manualSecondary.trim() || ready} onClick={()=>addSecondary.mutate()}>افزودن دستی</Button></div>}
        {!c.aiProviderId && <p className="text-xs text-amber-700">ارائه‌دهنده‌ی AI فعال نیست؛ ابتدا لایسنس را فعال کنید.</p>}
        {msg && <p className="text-sm text-slate-700">{msg}</p>}
        <ErrorBox message={error} />
        {(warnings.data?.length ?? 0) > 0 && (
          <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-xs text-amber-900">
            <div className="mb-1 font-semibold">هشدار شباهت ({num(warnings.data!.length)} مورد): این جفت‌ها ممکن است برای یک Search Intent باشند.</div>
            <ul className="space-y-0.5">
              {warnings.data!.slice(0, 8).map((w) => (
                <li key={w.aId + w.bId}>«{w.aKeyword}» ≈ «{w.bKeyword}» ({num(Math.round(w.score * 100))}٪)</li>
              ))}
            </ul>
            {!locked && <div className="mt-1 text-amber-700">با دکمه‌ی ✕ یکی از دو کلمه را حذف و سپس «تکمیل کمبود» را بزنید.</div>}
          </div>
        )}
      </Card>

      <div className="overflow-auto rounded-xl border border-slate-200 bg-white">
        <table className="w-full text-sm">
          <thead className="bg-slate-50 text-slate-600">
            <tr><th className="w-14 px-3 py-2 text-start">#</th><th className="px-3 py-2 text-start">کلمه‌ی ثانویه</th><th className="px-3 py-2 text-start">هدف جستجو</th><th className="px-3 py-2 text-start">منبع</th><th className="w-10" /></tr>
          </thead>
          <tbody>
            {items.map((k, i) => (
              <tr key={k.id} className={`border-t border-slate-100 ${flagged.has(k.id) ? "bg-amber-50" : ""}`}>
                <td className="px-3 py-2 text-slate-400">{num(i + 1)}</td>
                <td className="px-3 py-2">{editingSecondary?.id === k.id ? <div className="flex gap-1"><input className={inputClass} value={editingSecondary.value} onChange={e=>setEditingSecondary({...editingSecondary, value:e.target.value})}/><Button disabled={editSecondary.isPending} onClick={()=>editSecondary.mutate()}>ذخیره</Button><Button variant="ghost" onClick={()=>setEditingSecondary(null)}>لغو</Button></div> : k.keyword}</td>
                <td className="px-3 py-2 text-xs">{k.intent ? INTENT_FA[k.intent] : "—"}</td>
                <td className="px-3 py-2"><Badge tone={k.source === "ai" ? "amber" : "gray"}>{SOURCE_FA[k.source]}</Badge></td>
                <td className="px-2">{!locked && <div className="flex gap-2"><button title="ویرایش" className="text-brand-600" onClick={() => setEditingSecondary({id:k.id,value:k.keyword})}>✎</button><button title="حذف" className="text-slate-400 hover:text-red-600" onClick={() => remove.mutate(k.id)}>✕</button></div>}</td>
              </tr>
            ))}
            {items.length === 0 && <tr><td colSpan={5} className="px-3 py-8 text-center text-slate-500">هنوز کلمه‌ای تولید نشده است.</td></tr>}
          </tbody>
        </table>
      </div>

      <div className="flex flex-wrap gap-2">
        {locked ? (
          <>
            <Button onClick={next}>ادامه: عنوان‌ها</Button>
            <Button variant="secondary" onClick={() => setConfirmReset(true)}>بازنشانی برنامه‌ی مقالات</Button>
          </>
        ) : (
          <Button disabled={!ready || confirm.isPending} onClick={() => { setError(null); confirm.mutate(); }}>
            تأیید {num(c.targetCount)} کلمه و ساخت برنامه‌ی مقالات
          </Button>
        )}
      </div>

      {confirmReset && (
        <ConfirmDialog
          title="بازنشانی برنامه‌ی مقالات"
          message="برنامه‌ی مقالات و همه‌ی عنوان‌های تولید/تأییدشده حذف می‌شود (کلمات ثانویه می‌مانند). ادامه می‌دهید؟"
          confirmLabel="بازنشانی" busy={reset.isPending}
          onConfirm={() => reset.mutate()} onCancel={() => setConfirmReset(false)}
        />
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ step 4 */
function StepTitles({ c, onChanged, next }: { c: CampaignView; onChanged: () => void; next: () => void }) {
  const qc = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [busyPlan, setBusyPlan] = useState<string | null>(null);
  const [editing, setEditing] = useState<{ planId: string; value: string } | null>(null);
  const [manualTitles, setManualTitles] = useState("");

  const plans = useQuery({ queryKey: ["plans", c.id], queryFn: () => api.listArticlePlans(c.id) });
  const after = () => { qc.invalidateQueries({ queryKey: ["plans", c.id] }); onChanged(); };
  const fail = (e: unknown) => { setMsg(null); setError(errorMessage(e)); };

  const generate = useMutation({
    mutationFn: () => api.generateTitles(c.id),
    onSuccess: (r) => { setError(null); setMsg(r.missing.length ? `${num(r.generated)} عنوان ساخته شد؛ برای مقالات ${r.missing.map(num).join("، ")} عنوان ساخته نشد — دوباره تلاش کنید.` : `${num(r.generated)} عنوان ساخته شد.`); after(); },
    onError: fail,
  });
  const approveAll = useMutation({ mutationFn: () => api.approveAllTitles(c.id), onSuccess: after, onError: fail });
  const addBulkTitles = useMutation({
    mutationFn: async () => {
      const values = manualTitles.split(/\r?\n/).map(t => t.trim()).filter(Boolean);
      if (!values.length || values.some(t => [...t].length > 200)) throw new Error("برای هر عنوان یک خط (۱ تا ۲۰۰ نویسه) وارد کنید.");
      const normalized = values.map(t => t.toLocaleLowerCase("fa").replace(/\s+/g, " "));
      if (new Set(normalized).size !== normalized.length) throw new Error("عنوان تکراری در فهرست دستی وجود دارد.");
      const unfilled = (plans.data?.plans ?? []).filter(p => !p.title);
      if (values.length > unfilled.length) throw new Error(`فقط ${num(unfilled.length)} مقاله بدون عنوان وجود دارد. برای جایگزینی عنوان‌های موجود از دکمه ویرایش هر ردیف استفاده کنید.`);
      for (let i = 0; i < values.length; i++) await api.editTitle(unfilled[i].planId, values[i]);
      return values.length;
    },
    onSuccess: (n) => {setManualTitles("");setError(null);setMsg(`${num(n)} عنوان دستی افزوده و تأیید شد.`);after();},
    onError: (e) => {after();fail(e);},
  });

  const row = async (planId: string, fn: () => Promise<unknown>) => {
    setBusyPlan(planId); setError(null);
    try { await fn(); after(); } catch (e) { fail(e); } finally { setBusyPlan(null); }
  };

  const d = plans.data;
  if (c.planCount === 0) {
    return <Card><p className="text-sm text-slate-600">ابتدا در مرحله‌ی قبل کلمات ثانویه را تأیید کنید تا برنامه‌ی مقالات ساخته شود.</p></Card>;
  }

  return (
    <div className="space-y-4">
      <Card className="space-y-3">
        <div className="flex flex-wrap items-center gap-6">
          <div><div className="text-xs text-slate-500">تأییدشده</div><div className="text-xl font-bold text-emerald-700">{num(d?.approved ?? 0)} / {num(d?.total ?? 0)}</div></div>
          <div><div className="text-xs text-slate-500">در انتظار</div><div className="text-xl font-bold text-amber-700">{num(d?.pending ?? 0)} / {num(d?.total ?? 0)}</div></div>
          <div className="flex flex-wrap gap-2 ms-auto">
            <Button className="ai-btn" disabled={generate.isPending || !c.aiProviderId} onClick={() => { setMsg(null); generate.mutate(); }}>
              {generate.isPending ? "در حال تولید عنوان‌ها…" : "تولید عنوان‌ها"}
            </Button>
            <Button variant="secondary" disabled={approveAll.isPending} onClick={() => approveAll.mutate()}>تأیید همه‌ی عنوان‌های پیشنهادی</Button>
          </div>
        </div>
        <div className="rounded-lg border border-slate-200 p-3 space-y-2">
          <label className="block text-sm font-medium">افزودن گروهی عنوان‌های دستی برای مقالات بدون عنوان (هر خط یک عنوان)</label>
          <textarea rows={3} className={inputClass} value={manualTitles} onChange={e=>setManualTitles(e.target.value)} placeholder={"عنوان دلخواه مقاله اول\nعنوان دلخواه مقاله دوم"}/>
          <Button variant="secondary" disabled={addBulkTitles.isPending || !manualTitles.trim() || !d?.plans.some(p=>!p.title)} onClick={()=>addBulkTitles.mutate()}>{addBulkTitles.isPending ? "در حال افزودن…" : "افزودن و تأیید عنوان‌های دستی"}</Button>
          <p className="text-xs text-slate-500">عنوان‌های واردشده به ترتیب در اولین ردیف‌های بدون عنوان ثبت می‌شوند. برای ویرایش عنوان‌های موجود از دکمه ✎ همان ردیف استفاده کنید.</p>
        </div>
        {msg && <p className="text-sm text-slate-700">{msg}</p>}
        <ErrorBox message={error} />
        {d?.allApproved ? (
          <div className="flex flex-wrap items-center gap-3 rounded-lg border border-emerald-200 bg-emerald-50 p-3 text-sm text-emerald-800">
            <span>✓ همه‌ی {num(d.total)} عنوان تأیید شد.</span>
            <Button onClick={next}>ادامه و شروع تولید مقالات ←</Button>
          </div>
        ) : (
          <Button variant="secondary" disabled title="پس از تأیید همه‌ی عنوان‌ها فعال می‌شود">شروع تولید مقالات</Button>
        )}
      </Card>

      <div className="overflow-auto rounded-xl border border-slate-200 bg-white">
        <table className="w-full text-sm">
          <thead className="bg-slate-50 text-slate-600">
            <tr><th className="w-14 px-3 py-2 text-start">#</th><th className="px-3 py-2 text-start">کلمه‌ی ثانویه</th><th className="px-3 py-2 text-start">عنوان پیشنهادی</th><th className="px-3 py-2 text-start">هدف</th><th className="px-3 py-2 text-start">وضعیت</th><th className="px-3 py-2 text-start">عملیات</th></tr>
          </thead>
          <tbody>
            {d?.plans.map((p) => (
              <PlanRow key={p.planId} p={p} busy={busyPlan === p.planId} editing={editing?.planId === p.planId ? editing.value : null}
                onEditStart={() => setEditing({ planId: p.planId, value: p.title ?? "" })}
                onEditChange={(v) => setEditing({ planId: p.planId, value: v })}
                onEditCancel={() => setEditing(null)}
                onEditSave={() => editing && row(p.planId, async () => { await api.editTitle(p.planId, editing.value); setEditing(null); })}
                onApprove={() => p.titleId && row(p.planId, () => api.approveTitle(p.planId, p.titleId!))}
                onAlt={() => row(p.planId, () => api.generateAlternativeTitle(p.planId))}
                onReject={() => p.titleId && row(p.planId, () => api.rejectTitle(p.planId, p.titleId!))}
              />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function PlanRow(props: {
  p: PlanView; busy: boolean; editing: string | null;
  onEditStart: () => void; onEditChange: (v: string) => void; onEditCancel: () => void; onEditSave: () => void;
  onApprove: () => void; onAlt: () => void; onReject: () => void;
}) {
  const { p, busy, editing } = props;
  const approved = p.status === "TITLE_APPROVED";
  return (
    <tr className={`border-t border-slate-100 ${approved ? "bg-emerald-50/50" : ""}`}>
      <td className="px-3 py-2 text-slate-400">{num(p.articleNo)}</td>
      <td className="px-3 py-2 text-slate-600">{p.keyword}</td>
      <td className="min-w-[22rem] px-3 py-2">
        {editing !== null ? (
          <input className={inputClass} value={editing} autoFocus onChange={(e) => props.onEditChange(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") props.onEditSave(); if (e.key === "Escape") props.onEditCancel(); }} />
        ) : p.title ? p.title : <span className="text-slate-300">—</span>}
      </td>
      <td className="px-3 py-2 text-xs">{p.intent ? INTENT_FA[p.intent] : "—"}</td>
      <td className="px-3 py-2">{approved ? <Badge tone="green">تأییدشده</Badge> : p.title ? <Badge tone="amber">پیشنهادی</Badge> : <Badge tone="gray">بدون عنوان</Badge>}</td>
      <td className="whitespace-nowrap px-3 py-2">
        {editing !== null ? (
          <span className="flex gap-1"><Button disabled={busy} onClick={props.onEditSave}>ذخیره</Button><Button variant="ghost" onClick={props.onEditCancel}>انصراف</Button></span>
        ) : (
          <span className="flex gap-1">
            <button title="تأیید" disabled={busy || approved || !p.title} className="rounded px-2 py-1 hover:bg-emerald-100 disabled:opacity-30" onClick={props.onApprove}>✓</button>
            <button title="ویرایش یا افزودن عنوان دستی" disabled={busy} className="rounded px-2 py-1 hover:bg-slate-100" onClick={props.onEditStart}>✎</button>
            <button title="عنوان جایگزین" disabled={busy} className="rounded px-2 py-1 hover:bg-slate-100" onClick={props.onAlt}>{busy ? "…" : "↻"}</button>
            <button title="رد" disabled={busy || !p.title} className="rounded px-2 py-1 hover:bg-red-100 disabled:opacity-30" onClick={props.onReject}>🗑</button>
          </span>
        )}
      </td>
    </tr>
  );
}
