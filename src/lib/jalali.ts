// Jalali (Persian solar) calendar helpers. Conversions use the platform's Persian calendar (Intl),
// so no extra package is needed. All calendar-day arithmetic is done in UTC.

export const JALALI_MONTHS = ["فروردین", "اردیبهشت", "خرداد", "تیر", "مرداد", "شهریور", "مهر", "آبان", "آذر", "دی", "بهمن", "اسفند"];
const DAY = 86_400_000;
const fmtPersian = new Intl.DateTimeFormat("en-US-u-ca-persian", { year: "numeric", month: "numeric", day: "numeric", timeZone: "UTC" });

/** Jalali parts of a UTC calendar day. */
export function jalaliParts(utcDay: Date): { y: number; m: number; d: number } {
  const parts = fmtPersian.formatToParts(utcDay);
  const get = (t: string) => Number(parts.find((p) => p.type === t)?.value);
  return { y: get("year"), m: get("month"), d: get("day") };
}

/** UTC midnight of 1 Farvardin of the given Jalali year. */
function nowruz(jy: number): number {
  const base = Date.UTC(jy + 621, 2, 20);
  for (let k = -3; k <= 3; k++) {
    const t = base + k * DAY;
    const p = jalaliParts(new Date(t));
    if (p.y === jy && p.m === 1 && p.d === 1) return t;
  }
  return Date.UTC(jy + 621, 2, 21);
}

/** Jalali (y, m, d) to a UTC midnight timestamp. */
export function jalaliToUtcDay(jy: number, jm: number, jd: number): number {
  const offset = jm <= 6 ? (jm - 1) * 31 : 6 * 31 + (jm - 7) * 30;
  return nowruz(jy) + (offset + jd - 1) * DAY;
}

export function jalaliDaysInMonth(jy: number, jm: number): number {
  if (jm <= 6) return 31;
  if (jm <= 11) return 30;
  return (nowruz(jy + 1) - nowruz(jy)) / DAY === 366 ? 30 : 29;
}

/** Local date/time ISO (UTC, with Z) for a Jalali date and local hour/minute. */
export function jalaliToIso(jy: number, jm: number, jd: number, hh: number, mm: number): string {
  const u = new Date(jalaliToUtcDay(jy, jm, jd));
  return new Date(u.getUTCFullYear(), u.getUTCMonth(), u.getUTCDate(), hh, mm).toISOString();
}

/** Jalali parts and local time of an ISO timestamp. */
export function isoToJalali(iso: string): { y: number; m: number; d: number; hh: number; mm: number } {
  const local = new Date(iso);
  const p = jalaliParts(new Date(Date.UTC(local.getFullYear(), local.getMonth(), local.getDate())));
  return { ...p, hh: local.getHours(), mm: local.getMinutes() };
}

const fa = new Intl.NumberFormat("fa-IR", { useGrouping: false });
const pad2 = (n: number) => fa.format(n).padStart(2, "۰");

/** Readable Jalali date and time, e.g. ۱۴۰۵/۰۸/۰۳ ساعت ۱۰:۳۰ */
export function formatJalali(iso: string): string {
  const v = isoToJalali(iso);
  return `${fa.format(v.y)}/${pad2(v.m)}/${pad2(v.d)} ساعت ${pad2(v.hh)}:${pad2(v.mm)}`;
}

export function currentJalaliYear(): number {
  return isoToJalali(new Date().toISOString()).y;
}
