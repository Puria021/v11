import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { api, ArticleDetail } from "../api/tauri";
import { Button, ErrorBox, Field, inputClass } from "./ui";
import { errorMessage, num } from "../lib/format";
import ArticlePreviewBody from "./ArticlePreviewBody";

const PROSE =
  "space-y-3 text-sm leading-8 [&_h2]:mt-4 [&_h2]:text-lg [&_h2]:font-bold [&_h3]:font-semibold [&_li]:ms-5 [&_ol]:list-decimal [&_table]:border [&_td]:border [&_td]:p-1 [&_th]:border [&_th]:p-1 [&_ul]:list-disc [&_blockquote]:border-s-4 [&_blockquote]:ps-3 [&_a]:text-brand-600 [&_a]:underline";

/** Sanitized HTML preview, produced by the same Rust sanitizer that runs before upload. */
export function SafeHtml({ html }: { html: string }) {
  const q = useQuery({ queryKey: ["safeHtml", html], queryFn: () => api.previewHtml(html), staleTime: Infinity });
  return <div className={PROSE} dangerouslySetInnerHTML={{ __html: q.data ?? "" }} />;
}

export default function ArticleEditor({ planId, locked, onClose, onSaved }: { planId: string; locked: boolean; onClose: () => void; onSaved: () => void }) {
  const art = useQuery({ queryKey: ["article", planId], queryFn: () => api.getArticle(planId) });
  const [a, setA] = useState<ArticleDetail | null>(null);
  const [tab, setTab] = useState<"edit" | "preview">(locked ? "preview" : "edit");
  const [error, setError] = useState<string | null>(null);
  const [linkUrl, setLinkUrl] = useState<string | null>(null);
  const ta = useRef<HTMLTextAreaElement>(null);

  useEffect(() => { if (art.data && !a) setA(art.data); }, [art.data, a]);
  const save = useMutation({
    mutationFn: () => api.updateArticle(planId, {
      title: a!.title, slug: a!.slug, excerpt: a!.excerpt, contentHtml: a!.contentHtml,
      metaTitle: a!.metaTitle, metaDescription: a!.metaDescription, faq: a!.faq,
    }, a!.version),
    onSuccess: (r) => { setA(r); setError(null); onSaved(); },
    onError: (e) => setError(errorMessage(e)),
  });

  if (art.error) return <ErrorBox message={errorMessage(art.error)} />;
  if (!a) return <p className="text-sm text-slate-500">در حال بارگذاری…</p>;
  const set = <K extends keyof ArticleDetail>(k: K, v: ArticleDetail[K]) => setA({ ...a, [k]: v });
  const dirty = JSON.stringify(a) !== JSON.stringify(art.data);

  // Wrap the selected text (or insert a placeholder) with an HTML snippet.
  const wrap = (before: string, after: string, placeholder = "متن") => {
    const el = ta.current;
    if (!el) return;
    const [s, e] = [el.selectionStart, el.selectionEnd];
    const sel = a.contentHtml.slice(s, e) || placeholder;
    const next = a.contentHtml.slice(0, s) + before + sel + after + a.contentHtml.slice(e);
    set("contentHtml", next);
    requestAnimationFrame(() => { el.focus(); el.setSelectionRange(s + before.length, s + before.length + sel.length); });
  };
  const list = (tag: "ul" | "ol") => {
    const el = ta.current;
    const sel = el ? a.contentHtml.slice(el.selectionStart, el.selectionEnd) : "";
    const items = (sel || "مورد اول\nمورد دوم").split("\n").filter((l) => l.trim()).map((l) => `  <li>${l.trim()}</li>`).join("\n");
    wrap(`<${tag}>\n${items}\n</${tag}>`, "", "");
  };
  const tools: [string, string, () => void][] = [
    ["H2", "عنوان اصلی", () => wrap("<h2>", "</h2>", "عنوان")],
    ["H3", "زیرعنوان", () => wrap("<h3>", "</h3>", "زیرعنوان")],
    ["B", "پررنگ", () => wrap("<strong>", "</strong>")],
    ["I", "کج", () => wrap("<em>", "</em>")],
    ["• لیست", "لیست نقطه‌ای", () => list("ul")],
    ["1. لیست", "لیست عددی", () => list("ol")],
    ["❝", "نقل‌قول", () => wrap("<blockquote>", "</blockquote>")],
    ["</>", "کد", () => wrap("<code>", "</code>", "code")],
    ["جدول", "درج جدول", () => wrap("<table>\n  <thead><tr><th>ستون ۱</th><th>ستون ۲</th></tr></thead>\n  <tbody><tr><td>الف</td><td>ب</td></tr></tbody>\n</table>", "", "")],
    ["لینک", "درج لینک", () => setLinkUrl("https://")],
  ];

  return (
    <div className="space-y-4">
      <div className="flex gap-2">
        {!locked && <Button variant={tab === "edit" ? "primary" : "secondary"} onClick={() => setTab("edit")}>ویرایش</Button>}
        <Button variant={tab === "preview" ? "primary" : "secondary"} onClick={() => setTab("preview")}>پیش‌نمایش</Button>
        <span className="ms-auto self-center text-xs text-slate-500">{num(a.wordCount)} کلمه · نسخه {num(a.version)}</span>
      </div>
      {locked && <p className="rounded-lg bg-amber-50 p-2 text-xs text-amber-900">این مقاله در وردپرس ساخته شده یا در حال آپلود است؛ ویرایش محلی روی وردپرس اعمال نمی‌شود.</p>}

      {tab === "edit" && !locked && (
        <div className="space-y-4">
          <Field label="عنوان"><input className={inputClass} value={a.title} onChange={(e) => set("title", e.target.value)} /></Field>
          <div className="grid grid-cols-2 gap-3">
            <Field label="Slug" hint="فارسی هم مجاز است."><input className={`${inputClass} ltr`} value={a.slug} onChange={(e) => set("slug", e.target.value)} /></Field>
            <Field label="خلاصه (Excerpt)"><input className={inputClass} value={a.excerpt} onChange={(e) => set("excerpt", e.target.value)} /></Field>
            <Field label={`Meta Title (${num(a.metaTitle.length)}/۷۰)`}><input className={inputClass} value={a.metaTitle} onChange={(e) => set("metaTitle", e.target.value)} /></Field>
            <Field label={`Meta Description (${num(a.metaDescription.length)}/۱۷۰)`}><input className={inputClass} value={a.metaDescription} onChange={(e) => set("metaDescription", e.target.value)} /></Field>
          </div>

          <div>
            <div className="mb-1 flex flex-wrap items-center gap-1">
              {tools.map(([label, title, fn]) => (
                <button key={label} type="button" title={title} onClick={fn} className="rounded border border-slate-300 bg-white px-2 py-1 text-xs hover:bg-slate-50">{label}</button>
              ))}
            </div>
            {linkUrl !== null && (
              <div className="mb-2 flex gap-2">
                <input className={`${inputClass} ltr`} value={linkUrl} onChange={(e) => setLinkUrl(e.target.value)} autoFocus />
                <Button variant="secondary" onClick={() => { wrap(`<a href="${linkUrl.replace(/"/g, "&quot;")}">`, "</a>", "متن لینک"); setLinkUrl(null); }}>درج</Button>
                <Button variant="ghost" onClick={() => setLinkUrl(null)}>×</Button>
              </div>
            )}
            <textarea ref={ta} dir="ltr" className={`${inputClass} font-mono text-xs`} rows={18} value={a.contentHtml} onChange={(e) => set("contentHtml", e.target.value)} />
            <p className="mt-1 text-xs text-slate-500">HTML هنگام ذخیره پاک‌سازی می‌شود (اسکریپت، استایل، onclick و تصویر حذف می‌شوند؛ H1 به H2 تبدیل می‌شود).</p>
          </div>

          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium">پرسش‌های متداول (FAQ)</span>
              <Button variant="secondary" onClick={() => set("faq", [...a.faq, { q: "", a: "" }])}>+ پرسش</Button>
            </div>
            {a.faq.map((f, i) => (
              <div key={i} className="grid gap-2 rounded-lg border border-slate-200 p-2 md:grid-cols-[1fr_2fr_auto]">
                <input className={inputClass} placeholder="پرسش" value={f.q} onChange={(e) => set("faq", a.faq.map((x, j) => (j === i ? { ...x, q: e.target.value } : x)))} />
                <input className={inputClass} placeholder="پاسخ" value={f.a} onChange={(e) => set("faq", a.faq.map((x, j) => (j === i ? { ...x, a: e.target.value } : x)))} />
                <Button variant="ghost" className="text-red-600" onClick={() => set("faq", a.faq.filter((_, j) => j !== i))}>حذف</Button>
              </div>
            ))}
          </div>
        </div>
      )}

      {tab === "preview" && (
        <article className="space-y-4 rounded-xl border border-slate-200 bg-white p-5">
          <h1 className="text-2xl font-bold leading-10">{a.title}</h1>
          <ArticlePreviewBody planId={planId} html={a.contentHtml} />
          {a.faq.length > 0 && (
            <section>
              <h2 className="mb-2 mt-6 text-lg font-bold">پرسش‌های متداول</h2>
              {a.faq.filter((f) => f.q && f.a).map((f, i) => (<div key={i} className="mb-3"><h3 className="font-semibold">{f.q}</h3><p className="text-sm">{f.a}</p></div>))}
            </section>
          )}
        </article>
      )}

      {(a.validation?.warnings?.length ?? 0) > 0 && (
        <div className="rounded-lg border border-amber-200 bg-amber-50 p-2 text-xs text-amber-900">
          {a.validation!.warnings!.map((w) => <div key={w}>⚠ {w}</div>)}
        </div>
      )}
      <ErrorBox message={error} />
      <div className="flex justify-end gap-2">
        <Button variant="secondary" onClick={onClose}>بستن</Button>
        {!locked && <Button disabled={!dirty || save.isPending} onClick={() => { setError(null); save.mutate(); }}>{save.isPending ? "در حال ذخیره…" : "ذخیره"}</Button>}
      </div>
    </div>
  );
}
