import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";
import { api, BackupPreview } from "../api/tauri";
import { Badge, Button, Card, ErrorBox, inputClass } from "../components/ui";
import { errorMessage, num } from "../lib/format";
import Templates from "./Templates";
import Logs from "./Logs";
import Updates from "./Updates";
import { openUrl } from "@tauri-apps/plugin-opener";
import logo from "../assets/codeappmaster-logo.png";

const SITE_URL = "https://codeappmaster.ir";
// Text shown in the "درباره ما" tab. Edit the paragraphs here.
const ABOUT_PARAGRAPHS = [
  "Code App Master یک گروه نرم‌افزاری است که ابزارهای کاربردی برای طراحان وب و تولیدکنندگان محتوای فارسی می‌سازد.",
  "SEO Master یکی از محصولات این گروه است. با آن می‌توانید کمپین‌های مقاله و توضیحات محصول را برای سایت‌های وردپرس و ووکامرس تولید کنید.",
];

const TABS = [
  { id: "general", label: "عمومی" },
  { id: "license", label: "لایسنس" },
  { id: "updates", label: "بروزرسانی" },
  { id: "templates", label: "قالب‌ها" },
  { id: "logs", label: "گزارش‌ها" },
  { id: "runtime", label: "شبکه و صف" },
  { id: "cache", label: "کش" },
  { id: "about", label: "درباره ما" },
  { id: "backup", label: "پشتیبان‌گیری" },
  { id: "reset", label: "ریست" },
] as const;
type TabId = (typeof TABS)[number]["id"];

export default function Settings() {
  const [params, setParams] = useSearchParams();
  const requested = params.get("tab");
  const tab: TabId = TABS.some((t) => t.id === requested) ? (requested as TabId) : "general";
  const pick = (id: TabId) => setParams({ tab: id }, { replace: true });

  return (
    <div className="max-w-5xl space-y-6">
      <h1 className="text-2xl font-bold">تنظیمات</h1>
      <nav className="flex flex-wrap gap-2 border-b border-slate-200 pb-3">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            onClick={() => pick(t.id)}
            className={`rounded-lg border px-3 py-1.5 text-sm ${tab === t.id ? "border-brand-600 bg-brand-600 text-white" : "border-slate-200 bg-white text-slate-600 hover:bg-slate-100"}`}
          >
            {t.label}
          </button>
        ))}
      </nav>
      {tab === "general" && <AppInfo />}
      {tab === "license" && <LicenseCard />}
      {tab === "updates" && <Updates />}
      {tab === "templates" && <Templates />}
      {tab === "logs" && <Logs />}
      {tab === "runtime" && <><NetworkCard /><QueueCard /></>}
      {tab === "cache" && <CacheCard />}
      {tab === "about" && <AboutCard />}
      {tab === "backup" && <BackupCard />}
      {tab === "reset" && <ResetCard />}
    </div>
  );
}

function AppInfo() {
  const { data } = useQuery({ queryKey: ["appInfo"], queryFn: api.getAppInfo });
  return (
    <Card className="space-y-3 text-sm">
      <div className="flex justify-between"><span className="text-slate-500">نسخه‌ی برنامه</span><span>{data?.version ?? "—"}</span></div>
      <div className="flex justify-between gap-4"><span className="shrink-0 text-slate-500">مسیر داده‌ها</span><span className="ltr break-all text-xs">{data?.dataDir ?? "—"}</span></div>
      <p className="border-t border-slate-100 pt-3 text-xs text-slate-500">بروزرسانی خودکار هنوز فعال نیست؛ نسخه‌های جدید را به‌صورت نصب‌کننده دریافت کنید.</p>
    </Card>
  );
}

function LicenseCard() {
  const qc = useQueryClient();
  const status = useQuery({ queryKey: ["license"], queryFn: api.getLicenseStatus });
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const activate = useMutation({
    mutationFn: () => api.activateLicense(key.trim()),
    onSuccess: () => {
      setKey("");
      setError(null);
      setDone("لایسنس شما فعال شد");
      qc.invalidateQueries({ queryKey: ["license"] });
    },
    onError: (e) => { setDone(null); setError(errorMessage(e)); },
  });
  const submit = () => {
    setDone(null);
    if (!key.trim()) { setError("لایسنس را وارد کنید."); return; }
    setError(null);
    activate.mutate();
  };

  return (
    <Card className="max-w-xl space-y-4 text-sm">
      <div className="flex items-center justify-between">
        <span className="font-semibold">کلید لایسنس</span>
        {status.data?.activated ? <Badge tone="green">فعال</Badge> : <Badge tone="amber">فعال نشده</Badge>}
      </div>
      <div>
        <label className="mb-1 block text-sm font-medium">کلید API لایسنس</label>
        <input className={`${inputClass} ltr`} dir="ltr" type="password" autoComplete="off" placeholder={status.data?.activated ? "برای تغییر، کلید جدید را وارد کنید" : ""}
          value={key} onChange={(e) => setKey(e.target.value)} onKeyDown={(e) => e.key === "Enter" && !activate.isPending && submit()} />
      </div>
      <p className="text-xs leading-6 text-slate-500">
        پس از تأیید، اتصال متن و تصویر هم‌زمان آزمایش می‌شود. اگر هر دو موفق باشند، لایسنس فعال می‌شود. کلید در Credential Manager ویندوز ذخیره می‌شود و دوباره نمایش داده نمی‌شود.
      </p>
      <ErrorBox message={error} />
      {done && <p className="text-sm font-semibold text-emerald-700">{done}</p>}
      <Button disabled={activate.isPending} onClick={submit}>{activate.isPending ? "در حال تست اتصال…" : "تأیید"}</Button>
    </Card>
  );
}

function QueueCard() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["queueSettings"], queryFn: api.getQueueSettings });
  const [v, setV] = useState({ content: "", image: "", wordpress: "" });
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { if (q.data) setV({ content: String(q.data.content), image: String(q.data.image), wordpress: String(q.data.wordpress) }); }, [q.data]);
  const m = useMutation({
    mutationFn: () => api.updateQueueSettings(Number(v.content), Number(v.image), Number(v.wordpress)),
    onSuccess: () => { setError(null); qc.invalidateQueries({ queryKey: ["queueSettings"] }); },
    onError: (e) => setError(errorMessage(e)),
  });
  const f = (k: "content" | "image" | "wordpress", label: string) => (
    <label className="flex items-center gap-2 text-sm">{label}<input className={`${inputClass} ltr w-16`} value={v[k]} onChange={(e) => setV({ ...v, [k]: e.target.value })} /></label>
  );
  return (
    <Card className="space-y-3">
      <h2 className="font-semibold">همزمانی صف</h2>
      <div className="flex flex-wrap items-center gap-4">{f("content", "متن")}{f("image", "تصویر")}{f("wordpress", "وردپرس")}<Button variant="secondary" disabled={m.isPending} onClick={() => m.mutate()}>ذخیره</Button></div>
      <p className="text-xs text-slate-500">محدودیت درخواست در دقیقه برای هر سرویس، در صفحه‌ی ارائه‌دهندگان تنظیم می‌شود.</p>
      <ErrorBox message={error} />
    </Card>
  );
}

function BackupCard() {
  const qc = useQueryClient();
  const projects = useQuery({ queryKey: ["projects"], queryFn: api.listProjects });
  const [scope, setScope] = useState("");
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [file, setFile] = useState<string | null>(null);
  const [preview, setPreview] = useState<BackupPreview | null>(null);
  const [mode, setMode] = useState<"skip" | "replace">("skip");

  const doExport = useMutation({
    mutationFn: async () => {
      const path = await save({ defaultPath: "backup.aiproj", filters: [{ name: "SEO Master backup", extensions: ["aiproj"] }] });
      if (!path) return null;
      return api.exportBackup(path, scope || null);
    },
    onSuccess: (r) => { if (r) { setError(null); setMsg(`پشتیبان ساخته شد: ${num(r.projects)} پروژه، ${num(r.campaigns)} کمپین، ${num(r.articles)} مقاله، ${num(r.images)} تصویر (${num(Math.round(r.bytes / 1024))} کیلوبایت)`); } },
    onError: (e) => { setMsg(null); setError(errorMessage(e)); },
  });
  const pick = useMutation({
    mutationFn: async () => {
      const path = await open({ multiple: false, filters: [{ name: "SEO Master backup", extensions: ["aiproj"] }] });
      if (!path || Array.isArray(path)) return null;
      return { path, preview: await api.previewBackup(path) };
    },
    onSuccess: (r) => { if (r) { setFile(r.path); setPreview(r.preview); setError(null); setMsg(null); } },
    onError: (e) => { setMsg(null); setError(errorMessage(e)); },
  });
  const doImport = useMutation({
    mutationFn: () => api.importBackup(file!, mode),
    onSuccess: (r) => {
      setError(null);
      setMsg(`وارد شد: ${num(r.projectsImported)} پروژه، ${num(r.campaigns)} کمپین، ${num(r.articles)} مقاله، ${num(r.files)} تصویر.${r.projectsSkipped.length ? ` ردشده (از قبل موجود): ${r.projectsSkipped.join("، ")}` : ""}`);
      setPreview(null); setFile(null);
      qc.invalidateQueries();
    },
    onError: (e) => { setMsg(null); setError(errorMessage(e)); },
  });

  return (
    <Card className="space-y-4">
      <h2 className="font-semibold">پشتیبان‌گیری و بازیابی</h2>
      <p className="text-xs leading-6 text-slate-500">
        فایل پشتیبان (.aiproj) شامل پروژه‌ها، کمپین‌ها، کلمات، مقاله‌ها، قالب‌ها، تنظیمات غیرحساس و تصاویر است. <b>کلید API و رمز وردپرس هرگز در آن قرار نمی‌گیرد</b>؛
        بعد از بازیابی روی سیستم جدید باید دوباره وارد شوند.
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <select className={`${inputClass} w-56`} value={scope} onChange={(e) => setScope(e.target.value)}>
          <option value="">همه‌ی پروژه‌ها</option>
          {projects.data?.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
        </select>
        <Button variant="secondary" disabled={doExport.isPending} onClick={() => doExport.mutate()}>خروجی</Button>
        <Button variant="secondary" disabled={pick.isPending} onClick={() => pick.mutate()}>ورود…</Button>
      </div>
      {preview && (
        <div className="space-y-3 rounded-lg border border-slate-200 p-3 text-sm">
          <div className="font-semibold">پیش‌نمایش ورود</div>
          {preview.projects.map((p) => (
            <div key={p.id} className="flex items-center justify-between">
              <span>{p.name} — {num(p.campaigns)} کمپین، {num(p.articles)} مقاله</span>
              {p.exists ? <Badge tone="amber">از قبل موجود</Badge> : <Badge tone="green">جدید</Badge>}
            </div>
          ))}
          <div className="text-xs text-slate-500">قالب‌ها: {num(preview.templates)} · ارائه‌دهندگان AI: {num(preview.aiProviders)} · تصویر: {num(preview.imageProviders)} · تصاویر مقاله: {num(preview.images)}</div>
          <div className="flex flex-wrap items-center gap-3">
            <label className="flex items-center gap-1 text-sm"><input type="radio" checked={mode === "skip"} onChange={() => setMode("skip")} /> پروژه‌های موجود دست‌نخورده بمانند</label>
            <label className="flex items-center gap-1 text-sm"><input type="radio" checked={mode === "replace"} onChange={() => setMode("replace")} /> پروژه‌های موجود با پشتیبان جایگزین شوند</label>
          </div>
          {mode === "replace" && <p className="text-xs text-red-600">حالت جایگزینی، اطلاعات فعلی همان پروژه‌ها را حذف می‌کند.</p>}
          <div className="flex gap-2">
            <Button disabled={doImport.isPending} onClick={() => doImport.mutate()}>{doImport.isPending ? "در حال ورود…" : "ورود"}</Button>
            <Button variant="secondary" onClick={() => { setPreview(null); setFile(null); }}>انصراف</Button>
          </div>
        </div>
      )}
      {msg && <p className="text-sm text-emerald-700">{msg}</p>}
      <ErrorBox message={error} />
    </Card>
  );
}

function AboutCard() {
  return <Card className="space-y-4">
    <div className="flex flex-col items-center gap-3 text-center">
      <img src={logo} alt="لوگوی Code App Master" className="h-40 w-40 object-contain" />
      <h2 className="text-lg font-bold">Code App Master</h2>
    </div>
    {ABOUT_PARAGRAPHS.map((t) => <p key={t} className="text-sm leading-8 text-slate-700">{t}</p>)}
    <div className="flex items-center justify-center gap-2 border-t border-slate-100 pt-4 text-sm">
      <span>وب‌سایت:</span>
      <Button variant="ghost" onClick={() => openUrl(SITE_URL).catch(() => {})}><span className="ltr">codeappmaster.ir</span></Button>
    </div>
  </Card>;
}

function CacheCard() {
  const [msg, setMsg] = useState("");
  const [error, setError] = useState<string | null>(null);
  const clear = useMutation({
    mutationFn: () => api.clearCache(),
    onSuccess: (r) => {
      setError(null);
      const mb = (r.bytesFreed / (1024 * 1024)).toFixed(1);
      setMsg(`${num(r.filesRemoved)} فایل حذف شد و ${mb} مگابایت آزاد شد.${r.keptPending ? ` ${num(r.keptPending)} تصویر مقاله‌ی آماده‌ی آپلود نگه داشته شد.` : ""}`);
    },
    onError: (e) => { setMsg(""); setError(errorMessage(e)); },
  });
  return <Card className="space-y-3">
    <h2 className="font-semibold">کش برنامه</h2>
    <p className="text-sm leading-7 text-slate-600">تصاویر و فایل‌های موقت ذخیره‌شده در پوشه‌ی داده‌ی برنامه (AppData) را پاک می‌کند تا فضای دیسک آزاد شود. تصاویر مقاله‌هایی که هنوز آپلود نشده‌اند نگه داشته می‌شوند تا آپلودشان خراب نشود. پشتیبان‌های ذخیره‌شده پاک نمی‌شوند.</p>
    <Button variant="secondary" disabled={clear.isPending} onClick={() => { if (window.confirm("فایل‌های کش حذف شوند؟")) { setMsg(""); clear.mutate(); } }}>{clear.isPending ? "در حال حذف…" : "حذف کش"}</Button>
    {msg && <p className="text-sm text-emerald-700">{msg}</p>}
    <ErrorBox message={error} />
  </Card>;
}

function ResetCard() {
  const [open, setOpen] = useState(false);
  const [phrase, setPhrase] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const qc = useQueryClient();
  const reset = useMutation({ mutationFn: () => api.resetApplication(phrase), onSuccess: () => { setOpen(false); setPhrase(""); setErr(null); qc.clear(); window.location.reload(); }, onError: (e) => setErr(errorMessage(e)) });
  return <Card className="space-y-3">
    <h2 className="font-semibold text-red-700">بازنشانی کامل نرم‌افزار</h2>
    <p className="text-sm text-slate-600">تمام پروژه‌ها، کمپین‌ها، مقالات، تصاویر، ارائه‌دهندگان و کلیدهای ذخیره‌شده پاک می‌شوند. پیش از ادامه فایل پشتیبان بگیرید.</p>
    {!open ? <Button variant="secondary" onClick={() => setOpen(true)}>درخواست ریست کامل</Button> : <div className="space-y-3 rounded border border-red-300 p-3">
      <p className="text-sm font-semibold text-red-700">تأیید نهایی: برای پاک شدن داده‌ها عبارت RESET ALL را دقیقاً بنویسید.</p>
      <input className={`${inputClass} ltr`} value={phrase} onChange={(e) => setPhrase(e.target.value)} placeholder="RESET ALL" />
      <ErrorBox message={err} />
      <div className="flex gap-2"><Button variant="secondary" onClick={() => { setOpen(false); setPhrase(""); }}>انصراف</Button>
      <Button disabled={phrase !== "RESET ALL" || reset.isPending} onClick={() => { if (window.confirm("این عملیات برگشت‌ناپذیر است. تمام داده‌های برنامه حذف شوند؟")) reset.mutate(); }}>تأیید و حذف همه</Button></div>
    </div>}
  </Card>;
}

function NetworkCard() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["networkSettings"], queryFn: api.getNetworkSettings });
  const [proxyUrl, setProxyUrl] = useState("");
  const [dnsOverrides, setDnsOverrides] = useState("");
  const [msg, setMsg] = useState("");
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { if (q.data) { setProxyUrl(q.data.proxyUrl); setDnsOverrides(q.data.dnsOverrides); } }, [q.data]);
  const m = useMutation({ mutationFn: () => api.saveNetworkSettings({proxyUrl,dnsOverrides}),
    onSuccess: () => {setMsg("تنظیمات شبکه ذخیره و برای درخواست‌های جدید نرم‌افزار فعال شد.");setError(null);qc.invalidateQueries({queryKey:["networkSettings"]});},
    onError: e => {setMsg("");setError(errorMessage(e));}, });
  return <Card className="space-y-3">
    <h2 className="font-semibold">DNS و PROXY نرم‌افزار</h2>
    <p className="text-xs leading-6 text-slate-500">این تنظیمات فقط درخواست‌های HTTP برنامه (وردپرس، تولید متن و تصویر) را تحت تأثیر قرار می‌دهند و تنظیمات شبکه ویندوز را تغییر نمی‌دهند. پراکسی مخصوص هر سرویس بر پراکسی سراسری اولویت دارد.</p>
    <label className="block text-sm">Proxy سراسری (HTTP / HTTPS / SOCKS5)<input className={`${inputClass} block mt-1 ltr`} dir="ltr" placeholder="http://127.0.0.1:7890" value={proxyUrl} onChange={e=>setProxyUrl(e.target.value)} /></label>
    <label className="block text-sm">DNS Override (اتصال هر دامنه از طریق IP مشخص)<textarea className={`${inputClass} block mt-1 ltr min-h-28 font-mono`} dir="ltr" placeholder={"api.example.com=203.0.113.10\nsite.example.org=203.0.113.11"} value={dnsOverrides} onChange={e=>setDnsOverrides(e.target.value)} /></label>
    <p className="text-xs text-slate-500">هر خط: دامنه=IP یا دامنه=IP:PORT. پورت پیش‌فرض ۴۴۳ (HTTPS) است؛ برای HTTP پورت ۸۰ را بنویسید. برای استفاده از DNS Server دلخواه برای تمام دامنه‌ها، DNS را در ویندوز یا پراکسی خود تنظیم کنید؛ این قسمت IPهای سفارشی هر دامنه را اعمال می‌کند. با پراکسی بعضی نام‌گذاری‌ها در سرور پراکسی انجام می‌شود.</p>
    <Button disabled={m.isPending} onClick={()=>m.mutate()}>ذخیره تنظیمات شبکه</Button>
    {msg && <p className="text-sm text-emerald-700">{msg}</p>}<ErrorBox message={error}/>
  </Card>;
}
