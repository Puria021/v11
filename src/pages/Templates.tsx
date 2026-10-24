import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, Template } from "../api/tauri";
import { Button, Card, ErrorBox, Field, Modal, inputClass } from "../components/ui";
import { errorMessage, num } from "../lib/format";

// Only two templates exist: one for articles and one for product descriptions. They cannot be added or deleted.
const FIXED_LABEL: Record<string, string> = { tpl_default: "قالب مقاله", tpl_product: "قالب توضیحات محصول" };

export default function Templates() {
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: ["templates"], queryFn: api.listTemplates });
  const [editing, setEditing] = useState<Template | null>(null);
  const refresh = () => qc.invalidateQueries({ queryKey: ["templates"] });

  return (
    <div className="max-w-4xl space-y-6">
      <div>
        <h1 className="text-2xl font-bold">قالب‌ها</h1>
        <p className="mt-1 text-sm text-slate-500">دو قالب ثابت وجود دارد: برای مقالات و برای توضیحات محصولات. محتوای آن‌ها قابل ویرایش است، ولی قالب جدید نمی‌توان ساخت یا حذف کرد.</p>
      </div>
      <div className="grid gap-4 md:grid-cols-2">
        {data?.map((t) => (
          <Card key={t.id} className="space-y-2">
            <div className="font-semibold">{FIXED_LABEL[t.id ?? ""] ?? t.name}</div>
            <div className="text-xs text-slate-500">{num(t.minWords)}–{num(t.maxWords)} کلمه · FAQ: {t.faqEnabled ? num(t.faqCount) : "ندارد"} · تصویر: {num(t.imageCount)}</div>
            <div className="flex gap-2 border-t border-slate-100 pt-3">
              <Button variant="secondary" onClick={() => setEditing(t)}>ویرایش</Button>
            </div>
          </Card>
        ))}
      </div>
      {editing && <TemplateForm template={editing} onClose={() => setEditing(null)} onSaved={() => { setEditing(null); refresh(); }} />}
    </div>
  );
}

function TemplateForm({ template, onClose, onSaved }: { template: Template; onClose: () => void; onSaved: () => void }) {
  const [t, setT] = useState<Template>(template);
  const [error, setError] = useState<string | null>(null);
  const set = <K extends keyof Template>(k: K, v: Template[K]) => setT((s) => ({ ...s, [k]: v }));
  const save = useMutation({ mutationFn: () => api.saveTemplate(t), onSuccess: onSaved, onError: (e) => setError(errorMessage(e)) });
  const text = (k: keyof Template, label: string, rows = 3, hint?: string) => (
    <Field label={label} hint={hint}>
      <textarea className={inputClass} rows={rows} value={String(t[k] ?? "")} onChange={(e) => set(k, e.target.value as never)} />
    </Field>
  );
  const numField = (k: keyof Template, label: string) => (
    <Field label={label}>
      <input className={`${inputClass} ltr`} value={String(t[k] ?? "")} onChange={(e) => set(k, Number(e.target.value.replace(/\D/g, "")) as never)} />
    </Field>
  );

  return (
    <Modal title={t.id ? "ویرایش قالب" : "قالب جدید"} onClose={onClose}>
      <div className="space-y-4">
        <Field label="نام قالب"><input className={inputClass} value={t.name} onChange={(e) => set("name", e.target.value)} autoFocus /></Field>
        <p className="rounded-lg bg-slate-50 p-2 text-xs leading-6 text-slate-600">
          Placeholderها: <span className="ltr inline-block">{"{{title}} {{primary_keyword}} {{secondary_keyword}} {{brand_name}} {{target_audience}} {{internal_links}} {{faq}} {{phone}} {{phone_link}}"}</span>
        </p>
        {text("systemInstructions", "دستورالعمل سیستم (نقش و قوانین کلی)", 4)}
        {text("articleStructure", "ساختار مقاله", 4, "هر بخش را در یک خط بنویسید.")}
        {text("seoRules", "قوانین سئو", 3)}
        <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={t.internalLinking} onChange={(e) => set("internalLinking", e.target.checked)} /> فعال‌سازی لینک‌دهی داخلی فقط از آدرس‌های تأییدشده</label>
        {t.internalLinking && text("internalLinkRules", "قوانین لینک‌دهی داخلی", 5, "لینک ساختگی ممنوع است. اگر فهرست URL معتبر به مدل داده نشده، هیچ لینک داخلی ساخته نمی‌شود.")}
        {text("toneOfVoice", "لحن", 1)}
        {text("requiredSections", "بخش‌های اجباری", 2)}
        {text("brandInfo", "اطلاعات برند", 2)}
        {text("forbiddenTopics", "موضوعات ممنوع", 2)}
        {text("cta", "فراخوان به اقدام (CTA)", 2)}
        <div className="grid grid-cols-3 gap-3">
          {numField("minWords", "حداقل کلمه")}
          {numField("maxWords", "حداکثر کلمه")}
          {numField("maxRetries", "تلاش مجدد QC")}
        </div>
        <div className="grid grid-cols-2 gap-3">
          <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={t.faqEnabled} onChange={(e) => set("faqEnabled", e.target.checked)} /> بخش FAQ داشته باشد</label>
          {t.faqEnabled && numField("faqCount", "تعداد پرسش‌ها")}
          {t.faqEnabled && <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={t.faqSchema} onChange={(e) => set("faqSchema", e.target.checked)} /> افزودن اسکیما FAQPage به خروجی وردپرس</label>}
        </div>
        <div className="grid grid-cols-2 gap-3">
          <Field label="تعداد کل تصاویر (اولی تصویر شاخص)"><input type="number" min={0} max={10} className={`${inputClass} ltr`} value={t.imageCount} onChange={(e) => set("imageCount", Number(e.target.value))} /></Field>
          {t.imageCount > 1 && <Field label="فاصله تصاویر داخل مقاله (پاراگراف)"><input type="number" min={1} max={30} className={`${inputClass} ltr`} value={t.imageParagraphInterval} onChange={(e) => set("imageParagraphInterval", Number(e.target.value))} /></Field>}
          {t.imageCount > 0 && <Field label="اندازه‌ی تصویر" hint="مثل 1536x1024 (باید مورد پشتیبانی مدل باشد)"><input className={`${inputClass} ltr`} value={t.imageSize} onChange={(e) => set("imageSize", e.target.value)} /></Field>}
        </div>
        {t.imageCount > 0 && text("imageStyle", "سبک تصویر", 2)}
        <ErrorBox message={error} />
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onClose}>انصراف</Button>
          <Button disabled={save.isPending} onClick={() => { setError(null); save.mutate(); }}>{save.isPending ? "در حال ذخیره…" : "ذخیره"}</Button>
        </div>
      </div>
    </Modal>
  );
}
