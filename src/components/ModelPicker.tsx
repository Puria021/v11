import { useState } from "react";
import { inputClass } from "./ui";

export function ModelPicker({ value, onChange, models }: { value: string; onChange: (model: string) => void; models: string[] }) {
  const [search, setSearch] = useState("");
  const available = models.filter((m) => m.toLowerCase().includes(search.trim().toLowerCase()));
  const selectedOutsideList = value.trim() !== "" && !available.includes(value);

  return (
    <div className="space-y-2">
      <input className={`${inputClass} ltr`} value={search} onChange={(e) => setSearch(e.target.value)} placeholder="جستجو در مدل‌های پیشنهادی" aria-label="جستجوی مدل" />
      <select className={`${inputClass} ltr`} value={value} onChange={(e) => onChange(e.target.value)} aria-label="انتخاب مدل">
        <option value="">انتخاب مدل از فهرست</option>
        {selectedOutsideList && <option value={value}>{value} (مدل انتخاب‌شده)</option>}
        {available.map((m) => <option key={m} value={m}>{m}</option>)}
      </select>
      <input className={`${inputClass} ltr`} value={value} onChange={(e) => onChange(e.target.value)} placeholder="یا شناسه مدل را دستی وارد کنید" aria-label="ورود دستی شناسه مدل" spellCheck={false} />
      <p className="text-xs text-slate-500">مدل انتخابی یا شناسه واردشده ذخیره می‌شود؛ پشتیبانی آن به API ارائه‌دهنده بستگی دارد.</p>
    </div>
  );
}
