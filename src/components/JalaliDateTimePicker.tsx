import { inputClass } from "./ui";
import { currentJalaliYear, formatJalali, isoToJalali, jalaliDaysInMonth, jalaliToIso, JALALI_MONTHS } from "../lib/jalali";

type Parts = { y: number; m: number; d: number; hh: number; mm: number };

/** Controlled Jalali (solar) date and time picker. `onChange` receives an ISO UTC timestamp. */
export default function JalaliDateTimePicker({ value, onChange }: { value: string; onChange: (iso: string) => void }) {
  const p: Parts = value ? isoToJalali(value) : { ...isoToJalali(new Date().toISOString()), hh: 9, mm: 0 };
  const thisYear = currentJalaliYear();
  const days = jalaliDaysInMonth(p.y, p.m);
  const emit = (next: Partial<Parts>) => {
    const n = { ...p, ...next };
    onChange(jalaliToIso(n.y, n.m, Math.min(n.d, jalaliDaysInMonth(n.y, n.m)), n.hh, n.mm));
  };
  const sel = `${inputClass} w-full`;
  const years = Array.from(new Set([thisYear - 1, thisYear, thisYear + 1, thisYear + 2, p.y])).sort((a, b) => a - b);
  return (
    <div className="space-y-3">
      <div className="grid grid-cols-3 gap-2">
        <label className="text-xs text-slate-600">روز
          <select className={sel} value={Math.min(p.d, days)} onChange={(e) => emit({ d: Number(e.target.value) })}>
            {Array.from({ length: days }, (_, i) => i + 1).map((n) => <option key={n} value={n}>{n}</option>)}
          </select>
        </label>
        <label className="text-xs text-slate-600">ماه
          <select className={sel} value={p.m} onChange={(e) => emit({ m: Number(e.target.value) })}>
            {JALALI_MONTHS.map((name, i) => <option key={name} value={i + 1}>{name}</option>)}
          </select>
        </label>
        <label className="text-xs text-slate-600">سال
          <select className={sel} value={p.y} onChange={(e) => emit({ y: Number(e.target.value) })}>
            {years.map((n) => <option key={n} value={n}>{n}</option>)}
          </select>
        </label>
        <label className="text-xs text-slate-600">ساعت
          <select className={sel} value={p.hh} onChange={(e) => emit({ hh: Number(e.target.value) })}>
            {Array.from({ length: 24 }, (_, i) => i).map((n) => <option key={n} value={n}>{String(n).padStart(2, "0")}</option>)}
          </select>
        </label>
        <label className="text-xs text-slate-600">دقیقه
          <select className={sel} value={p.mm} onChange={(e) => emit({ mm: Number(e.target.value) })}>
            {Array.from({ length: 60 }, (_, i) => i).map((n) => <option key={n} value={n}>{String(n).padStart(2, "0")}</option>)}
          </select>
        </label>
      </div>
      {value && <p className="text-sm text-slate-700">زمان انتخاب‌شده: <span className="font-semibold">{formatJalali(value)}</span></p>}
    </div>
  );
}
