import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, ArticleRow, CampaignView } from "../api/tauri";
import { Badge, Button, Card, ConfirmDialog, ErrorBox, Modal, inputClass } from "./ui";
import ArticleEditor from "./ArticleEditor";
import { STATUS } from "./StepGenerate";
import { errorMessage, num } from "../lib/format";

export default function StepReview({ c, onChanged }: { c: CampaignView; onChanged: () => void }) {
  const qc = useQueryClient();
  const articles = useQuery({
    queryKey: ["articles", c.id],
    queryFn: () => api.listArticles(c.id),
    refetchInterval: (q) => (q.state.data?.some((a) => ["UPLOADING", "QUEUED", "GENERATING", "IMAGE_GENERATING"].includes(a.status)) ? 2500 : false),
  });
  const progress = useQuery({ queryKey: ["progress", c.id], queryFn: () => api.getProgress(c.id) });
  const terms = useQuery({ queryKey: ["terms", c.projectId, "category"], queryFn: () => api.listWpTerms(c.projectId, "category") });
  const conn = useQuery({ queryKey: ["wp", c.projectId], queryFn: () => api.getWpConnection(c.projectId) });
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<ArticleRow | null>(null);
  const [taxFor, setTaxFor] = useState<ArticleRow | null>(null);
  const [confirmAll, setConfirmAll] = useState(false);
  const [bulkAt, setBulkAt] = useState("");
  const [scheduleMsg, setScheduleMsg] = useState("");
  const bulkSchedule = useMutation({
    mutationFn: () => api.setUploadSchedule(c.id, null, bulkAt ? new Date(bulkAt).toISOString() : null),
    onSuccess: (n) => { setScheduleMsg(`زمان انتشار ${num(n)} مقاله در وردپرس ثبت شد.`); setError(null); after(); },
    onError: failSchedule,
  });
  function failSchedule(e: unknown) { setScheduleMsg(""); setError(errorMessage(e)); }


  const after = () => {
    qc.invalidateQueries({ queryKey: ["articles", c.id] });
    qc.invalidateQueries({ queryKey: ["progress", c.id] });
    onChanged();
  };
  const fail = (e: unknown) => setError(errorMessage(e));
  const uploadAll = useMutation({ mutationFn: () => api.uploadCampaign(c.id), onSuccess: () => { setConfirmAll(false); setError(null); after(); }, onError: (e) => { setConfirmAll(false); fail(e); } });
  const uploadOne = useMutation({
    mutationFn: ({ id, publish }: { id: string; publish: boolean }) => api.uploadToWordpress(id, publish),
    onSuccess: () => { setError(null); after(); },
    onError: fail,
  });

  const rows = articles.data ?? [];
  const ready = rows.filter((a) => a.status === "READY_TO_UPLOAD").length;
  const drafts = rows.filter((a) => a.status === "DRAFT_CREATED").length;
  const failed = rows.filter((a) => a.status === "FAILED").length;
  const p = progress.data;
  const connected = conn.data?.status === "CONNECTED";

  return (
    <div className="space-y-4">
      <Card className="space-y-3">
        <div className="flex flex-wrap items-center gap-6">
          <Stat label="آماده‌ی آپلود" v={`${num(ready)}`} />
          <Stat label="پیش‌نویس ساخته‌شده" v={`${num(drafts)} / ${num(rows.length)}`} tone="green" />
          <Stat label="ناموفق" v={num(failed)} tone={failed ? "red" : undefined} />
          <div className="ms-auto flex flex-wrap gap-2">
            {failed > 0 && <RetryFailed campaignId={c.id} onDone={after} onError={fail} />}
            <Button disabled={ready === 0 || uploadAll.isPending} onClick={() => setConfirmAll(true)}>ساخت پیش‌نویس‌های وردپرس ({num(ready)})</Button>
          </div>
        </div>
        {p && (
          <div>
            <div className="mb-1 flex justify-between text-xs text-slate-500"><span>وردپرس</span><span>{num(p.wordpress.done)} / {num(p.wordpress.total)}</span></div>
            <div className="h-2 overflow-hidden rounded-full bg-slate-100"><div className="h-full rounded-full bg-emerald-500 transition-all" style={{ width: `${p.wordpress.total ? (p.wordpress.done * 100) / p.wordpress.total : 0}%` }} /></div>
          </div>
        )}
        {!connected && <p className="text-xs text-amber-700">اتصال وردپرس این پروژه هنوز «تست‌شده و متصل» نیست؛ از صفحه‌ی پروژه اتصال را تست کنید. همه‌ی مقالات فقط به صورت Draft ساخته می‌شوند.</p>}
        <p className="text-xs text-slate-500">متا تایتل و متادسکریپشن به Rank Math ارسال می‌شود. برای آن و بخش محصولات باید افزونه «SEO Master Bridge» را روی وردپرس فعال کنید.</p>
        <div className="flex flex-wrap gap-2 items-end border-t border-slate-100 pt-3"><label className="text-sm">زمان انتشار گروهی در وردپرس (زمان محلی سیستم)<input type="datetime-local" className={`${inputClass} block mt-1 ltr`} value={bulkAt} onChange={e=>setBulkAt(e.target.value)}/></label><Button variant="secondary" disabled={bulkSchedule.isPending} onClick={()=>bulkSchedule.mutate()}>ثبت برای همه</Button><Button variant="ghost" onClick={()=>{setBulkAt("");api.setUploadSchedule(c.id,null,null).then(()=>{setScheduleMsg("تاریخ‌های انتشار پاک شد.");after();}).catch(failSchedule);}}>پاک کردن تاریخ‌ها</Button></div>
        <p className="text-xs text-slate-500">تاریخ انتشار، تاریخ نوشته در وردپرس است و آپلود را به تأخیر نمی‌اندازد. نوشته به صورت پیش‌نویس با این تاریخ ساخته می‌شود؛ برای انتشار فوری از دکمه‌ی «انتشار مستقیم» در هر ردیف استفاده کنید.</p>
        {scheduleMsg && <p className="text-sm text-emerald-700">{scheduleMsg}</p>}
        <ErrorBox message={error} />
      </Card>

      <div className="overflow-auto rounded-xl border border-slate-200 bg-white">
        <table className="w-full text-sm">
          <thead className="bg-slate-50 text-slate-600">
            <tr><th className="w-14 px-3 py-2 text-start">#</th><th className="px-3 py-2 text-start">عنوان</th><th className="px-3 py-2 text-start">وضعیت</th><th className="px-3 py-2 text-start">دسته / برچسب</th><th className="px-3 py-2 text-start">عملیات</th></tr>
          </thead>
          <tbody>
            {rows.map((a) => {
              const [label, tone] = STATUS[a.status] ?? [a.status, "gray" as const];
              const catNames = (terms.data ?? []).filter((t) => a.categoryIds.includes(t.wpId)).map((t) => t.name);
              const done = a.status === "DRAFT_CREATED";
              return (
                <tr key={a.planId} className="border-t border-slate-100 align-top">
                  <td className="px-3 py-2 text-slate-400">{num(a.articleNo)}</td>
                  <td className="px-3 py-2">
                    <div>{a.title ?? a.keyword} <span className="text-xs text-slate-500">· کلمه‌ی کلیدی: {a.keyword}</span></div>
                    {a.lastError && a.status === "FAILED" && <div className="mt-1 text-xs text-red-600">{a.lastError}</div>}
                  </td>
                  <td className="px-3 py-2"><Badge tone={tone}>{label}</Badge></td>
                  <td className="px-3 py-2 text-xs text-slate-600">
                    <div>{catNames.length ? catNames.join("، ") : <span className="text-slate-300">دسته‌ی پیش‌فرض</span>}</div>
                    <div>{a.tags.length ? a.tags.join("، ") : <span className="text-slate-300">بدون برچسب</span>}</div>
                  </td>
                  <td className="whitespace-nowrap px-3 py-2">
                    <div className="flex flex-wrap gap-1">
                      <Button variant="ghost" disabled={a.wordCount === null} onClick={() => setEditing(a)}>{done ? "پیش‌نمایش" : "ویرایش / پیش‌نمایش"}</Button>
                      {!done && a.wordCount !== null && <Button variant="ghost" onClick={() => setTaxFor(a)}>دسته و برچسب</Button>}
                      {!done && a.wordCount !== null && <UploadScheduleControl key={a.planId + (a.uploadAt ?? "")} a={a} campaignId={c.id} onChanged={after} onError={fail} />}
                      {(a.status === "READY_TO_UPLOAD" || (a.status === "FAILED" && a.wordCount !== null)) && (
                        <>
                          <Button variant="secondary" disabled={uploadOne.isPending} onClick={() => uploadOne.mutate({ id: a.planId, publish: false })}>{a.status === "FAILED" ? "تلاش مجدد آپلود" : "آپلود به وردپرس"}</Button>
                          <Button variant="danger" disabled={uploadOne.isPending} onClick={() => { if (window.confirm(`مقاله‌ی ${num(a.articleNo)} بلافاصله در سایت منتشر می‌شود. ادامه می‌دهید؟`)) uploadOne.mutate({ id: a.planId, publish: true }); }}>انتشار مستقیم</Button>
                        </>
                      )}
                      {done && a.wordpressUrl && <Button variant="ghost" onClick={() => openUrl(a.wordpressUrl!).catch(fail)}>مشاهده‌ی پیش‌نویس در وردپرس</Button>}
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {confirmAll && (
        <ConfirmDialog title="ساخت پیش‌نویس در وردپرس" message={`${num(ready)} مقاله به صورت پیش‌نویس در وردپرس ساخته می‌شود. چیزی منتشر نمی‌شود و آپلود تکراری مسدود است. ادامه می‌دهید؟`}
          confirmLabel="ساخت پیش‌نویس‌ها" busy={uploadAll.isPending} onConfirm={() => uploadAll.mutate()} onCancel={() => setConfirmAll(false)} />
      )}
      {editing && (
        <Modal title={`مقاله‌ی ${num(editing.articleNo)}`} onClose={() => setEditing(null)} wide>
          <ArticleEditor planId={editing.planId} locked={["DRAFT_CREATED", "UPLOADING"].includes(editing.status)} onClose={() => setEditing(null)} onSaved={after} />
        </Modal>
      )}
      {taxFor && <TaxonomyDialog a={taxFor} projectId={c.projectId} onClose={() => setTaxFor(null)} onSaved={() => { setTaxFor(null); after(); }} />}
    </div>
  );
}

function UploadScheduleControl({a,campaignId,onChanged,onError}: {a:ArticleRow;campaignId:string;onChanged:()=>void;onError:(e:unknown)=>void}) {
  const local = a.uploadAt ? (()=>{ const d=new Date(a.uploadAt); return new Date(d.getTime()-d.getTimezoneOffset()*60000).toISOString().slice(0,16);})() : "";
  const [at,setAt] = useState(local);
  const mutation = useMutation({mutationFn:()=>api.setUploadSchedule(campaignId,a.planId,at ? new Date(at).toISOString():null),onSuccess:onChanged,onError});
  return <div className="inline-flex items-center gap-1"><input aria-label={`زمان انتشار مقاله ${a.articleNo}`} type="datetime-local" className={`${inputClass} ltr text-xs max-w-44`} value={at} onChange={e=>setAt(e.target.value)}/><Button variant="ghost" disabled={mutation.isPending} onClick={()=>mutation.mutate()}>ثبت زمان انتشار</Button></div>;
}

function Stat({ label, v, tone }: { label: string; v: string; tone?: "red" | "green" }) {
  const color = tone === "red" ? "text-red-600" : tone === "green" ? "text-emerald-700" : "text-brand-700";
  return <div><div className="text-xs text-slate-500">{label}</div><div className={`text-xl font-bold ${color}`}>{v}</div></div>;
}

function RetryFailed({ campaignId, onDone, onError }: { campaignId: string; onDone: () => void; onError: (e: unknown) => void }) {
  const m = useMutation({ mutationFn: () => api.retryFailed(campaignId), onSuccess: onDone, onError });
  return <Button variant="secondary" disabled={m.isPending} onClick={() => m.mutate()}>تلاش مجدد برای ناموفق‌ها</Button>;
}

function TaxonomyDialog({ a, projectId, onClose, onSaved }: { a: ArticleRow; projectId: string; onClose: () => void; onSaved: () => void }) {
  const terms = useQuery({ queryKey: ["terms", projectId, "category"], queryFn: () => api.listWpTerms(projectId, "category") });
  const [cats, setCats] = useState<number[]>(a.categoryIds);
  const [tags, setTags] = useState(a.tags.join("، "));
  const [error, setError] = useState<string | null>(null);
  const save = useMutation({
    mutationFn: () => api.setPlanTaxonomy(a.planId, cats, tags.split(/[,،\n]/).map((t) => t.trim()).filter(Boolean)),
    onSuccess: onSaved,
    onError: (e) => setError(errorMessage(e)),
  });
  const toggle = (id: number) => setCats((s) => (s.includes(id) ? s.filter((x) => x !== id) : [...s, id]));
  return (
    <Modal title={`دسته و برچسب — مقاله‌ی ${num(a.articleNo)}`} onClose={onClose}>
      <div className="space-y-4">
        <div>
          <div className="mb-1 text-sm font-medium">دسته‌بندی‌ها</div>
          {(terms.data?.length ?? 0) === 0 ? (
            <p className="text-xs text-slate-500">دسته‌ای همگام نشده است. در صفحه‌ی پروژه «دریافت دسته‌ها و برچسب‌ها» را بزنید؛ در غیر این صورت دسته‌ی پیش‌فرض وردپرس استفاده می‌شود.</p>
          ) : (
            <div className="max-h-48 space-y-1 overflow-auto rounded-lg border border-slate-200 p-2">
              {terms.data!.map((t) => (
                <label key={t.wpId} className="flex items-center gap-2 text-sm"><input type="checkbox" checked={cats.includes(t.wpId)} onChange={() => toggle(t.wpId)} /> {t.name}</label>
              ))}
            </div>
          )}
          <p className="mt-1 text-xs text-slate-500">بدون انتخاب، دسته‌ی پیش‌فرض پروژه (در صفحه‌ی پروژه) استفاده می‌شود.</p>
        </div>
        <div>
          <label className="mb-1 block text-sm font-medium">برچسب‌ها (با ویرگول جدا کنید)</label>
          <input className={inputClass} value={tags} onChange={(e) => setTags(e.target.value)} />
          <Button variant="ghost" className="mt-1" onClick={() => setTags((t) => (t ? `${t}، ${a.keyword}` : a.keyword))}>+ کلمه‌ی کلیدی «{a.keyword}»</Button>
        </div>
        <ErrorBox message={error} />
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onClose}>انصراف</Button>
          <Button disabled={save.isPending} onClick={() => { setError(null); save.mutate(); }}>ذخیره</Button>
        </div>
      </div>
    </Modal>
  );
}
