import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { api, ProjectCard, ProjectInput } from "../api/tauri";
import { Button, ErrorBox, Field, Modal, inputClass } from "./ui";
import { errorMessage } from "../lib/format";

export default function ProjectForm({
  project,
  onClose,
  onSaved,
}: {
  project: ProjectCard | null;
  onClose: () => void;
  onSaved: (p: ProjectCard) => void;
}) {
  const [f, setF] = useState<ProjectInput>({
    name: project?.name ?? "",
    description: project?.description ?? "",
    siteUrl: project?.siteUrl ?? "",
    brandName: project?.brandName ?? "",
    logoDataUrl: project?.logoDataUrl ?? "",
    targetAudience: project?.targetAudience ?? "",
    phone: project?.phone ?? "",
    defaultArticleCount: project?.defaultArticleCount ?? 60,
    defaultCountry: project?.defaultCountry ?? "IR",
    defaultLanguage: project?.defaultLanguage ?? "fa",
  });
  const [error, setError] = useState<string | null>(null);

  const save = useMutation({
    mutationFn: () => (project ? api.updateProject(project.id, f) : api.createProject(f)),
    onSuccess: onSaved,
    onError: (e) => setError(errorMessage(e)),
  });

  const set = <K extends keyof ProjectInput>(k: K, v: ProjectInput[K]) => setF((s) => ({ ...s, [k]: v }));

  return (
    <Modal title={project ? "ویرایش پروژه" : "افزودن پروژه"} onClose={onClose}>
      <div className="space-y-4">
        <Field label="نام پروژه">
          <input className={inputClass} value={f.name} onChange={(e) => set("name", e.target.value)} autoFocus />
        </Field>
        <Field label="آدرس سایت" hint="مثال: https://example.com">
          <input className={`${inputClass} ltr`} value={f.siteUrl} onChange={(e) => set("siteUrl", e.target.value)} />
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="شماره تماس برای CTA"><input className={`${inputClass} ltr`} type="tel" value={f.phone} onChange={(e) => set("phone", e.target.value)} /></Field>
          <Field label="تعداد پیش‌فرض مقالات هر کمپین"><input className={`${inputClass} ltr`} type="number" min={1} max={200} value={f.defaultArticleCount} onChange={(e) => set("defaultArticleCount", Number(e.target.value))} /></Field>
        </div>
        <Field label="لوگوی پروژه برای واترمارک" hint="PNG، JPG یا WebP؛ حداکثر ۲ مگابایت. تصویر شفاف PNG پیشنهاد می‌شود.">
          <input className={inputClass} type="file" accept="image/png,image/jpeg,image/webp" onChange={(e) => {
            const file = e.target.files?.[0];
            if (!file) return;
            if (file.size > 2 * 1024 * 1024) { setError("حجم لوگو باید حداکثر ۲ مگابایت باشد."); return; }
            const reader = new FileReader();
            reader.onload = () => set("logoDataUrl", String(reader.result ?? ""));
            reader.readAsDataURL(file);
          }} />
          {f.logoDataUrl && <div className="flex items-center gap-3 mt-2"><img src={f.logoDataUrl} alt="پیش‌نمایش لوگو" className="max-h-16 max-w-32 object-contain" /><Button variant="secondary" onClick={() => set("logoDataUrl", "")}>حذف لوگو</Button></div>}
        </Field>
        <Field label="توضیحات">
          <textarea className={inputClass} rows={2} value={f.description} onChange={(e) => set("description", e.target.value)} />
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="نام برند">
            <input className={inputClass} value={f.brandName} onChange={(e) => set("brandName", e.target.value)} />
          </Field>
          <Field label="مخاطب هدف">
            <input className={inputClass} value={f.targetAudience} onChange={(e) => set("targetAudience", e.target.value)} />
          </Field>
          <Field label="کشور پیش‌فرض" hint="کد دو حرفی، مثل IR">
            <input className={`${inputClass} ltr`} maxLength={2} value={f.defaultCountry} onChange={(e) => set("defaultCountry", e.target.value)} />
          </Field>
          <Field label="زبان پیش‌فرض" hint="مثل fa یا en">
            <input className={`${inputClass} ltr`} maxLength={8} value={f.defaultLanguage} onChange={(e) => set("defaultLanguage", e.target.value)} />
          </Field>
        </div>
        <ErrorBox message={error} />
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={onClose} disabled={save.isPending}>
            انصراف
          </Button>
          <Button onClick={() => { setError(null); save.mutate(); }} disabled={save.isPending}>
            {save.isPending ? "در حال ذخیره…" : "ذخیره"}
          </Button>
        </div>
      </div>
    </Modal>
  );
}
