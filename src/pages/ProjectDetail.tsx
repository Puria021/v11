import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, SyncResult, TestResult } from "../api/tauri";
import { Badge, Button, Card, ErrorBox, Field, inputClass } from "../components/ui";
import { errorMessage, faDate, num } from "../lib/format";
import { WpBadge } from "./Dashboard";

export default function ProjectDetail({ online }: { online: boolean | null }) {
  const { id = "" } = useParams();
  const qc = useQueryClient();
  const project = useQuery({ queryKey: ["project", id], queryFn: () => api.getProject(id) });
  const conn = useQuery({ queryKey: ["wp", id], queryFn: () => api.getWpConnection(id) });

  const [apiUrl, setApiUrl] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [proxy, setProxy] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [testResult, setTestResult] = useState<TestResult | null>(null);
  const [syncResult, setSyncResult] = useState<SyncResult | null>(null);

  // Fill the form from the stored connection, or default the API URL from the site URL.
  useEffect(() => {
    if (conn.data) {
      setApiUrl(conn.data.apiUrl);
      setUsername(conn.data.username);
      setProxy(conn.data.proxyUrl ?? "");
    } else if (conn.data === null && project.data) {
      setApiUrl(project.data.siteUrl);
    }
  }, [conn.data, project.data]);

  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["wp", id] });
    qc.invalidateQueries({ queryKey: ["project", id] });
    qc.invalidateQueries({ queryKey: ["projects"] });
  };

  const save = useMutation({
    mutationFn: () =>
      api.saveWpConnection({ projectId: id, apiUrl, username, applicationPassword: password || undefined, proxyUrl: proxy || undefined }),
    onSuccess: () => {
      setPassword("");
      setTestResult(null);
      refresh();
    },
    onError: (e) => setError(errorMessage(e)),
  });

  const test = useMutation({
    mutationFn: () => api.testWpConnection(id),
    onSuccess: (r) => {
      setTestResult(r);
      refresh();
    },
    onError: (e) => {
      setTestResult(null);
      setError(errorMessage(e));
      refresh();
    },
  });

  const categories = useQuery({ queryKey: ["terms", id, "category"], queryFn: () => api.listWpTerms(id, "category") });
  const setDefaultCat = useMutation({
    mutationFn: (v: string) => api.setDefaultCategory(id, v ? Number(v) : null),
    onSuccess: refresh,
    onError: (e) => setError(errorMessage(e)),
  });
  const sync = useMutation({
    mutationFn: () => api.syncWpTaxonomies(id),
    onSuccess: (r) => {
      setSyncResult(r);
      qc.invalidateQueries({ queryKey: ["terms", id, "category"] });
      refresh();
    },
    onError: (e) => setError(errorMessage(e)),
  });

  if (project.isLoading) return <p className="text-sm text-slate-500">در حال بارگذاری…</p>;
  if (project.error || !project.data) return <ErrorBox message={errorMessage(project.error ?? "پروژه پیدا نشد.")} />;
  const p = project.data;
  const c = conn.data;
  const offline = online === false;
  const busy = save.isPending || test.isPending || sync.isPending;

  const run = (m: { mutate: () => void }) => () => {
    setError(null);
    m.mutate();
  };

  return (
    <div className="max-w-3xl space-y-6">
      <div>
        <Link to="/projects" className="text-sm text-brand-600">
          → بازگشت به پروژه‌ها
        </Link>
        <div className="mt-2 flex items-center gap-3">
          <h1 className="text-2xl font-bold">{p.name}</h1>
          <WpBadge status={p.wpStatus} />
        </div>
        <div className="ltr mt-1 text-sm text-slate-500">{p.siteUrl}</div>
        {p.description && <p className="mt-2 text-sm text-slate-600">{p.description}</p>}
      </div>

      <Card className="space-y-4">
        <h2 className="text-lg font-semibold">اتصال به وردپرس</h2>
        <p className="text-xs leading-6 text-slate-500">
          افزونه‌ی «SEO Master Bridge» را در وردپرس نصب کنید، از تنظیمات ← SEO Master Bridge توکن بسازید و آن را
          اینجا وارد کنید. توکن در Windows Credential Manager ذخیره می‌شود و دوباره نمایش داده نمی‌شود. اگر افزونه ندارید،
          می‌توانید نام کاربری و Application Password را به‌جای آن وارد کنید.
        </p>

        <Field label="آدرس سایت یا API" hint="آدرس سایت کافی است؛ /wp-json خودکار اضافه می‌شود.">
          <input className={`${inputClass} ltr`} value={apiUrl} onChange={(e) => setApiUrl(e.target.value)} />
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="نام کاربری" hint="فقط برای حالت Application Password. با توکن افزونه خالی بگذارید.">
            <input className={`${inputClass} ltr`} value={username} onChange={(e) => setUsername(e.target.value)} autoComplete="off" />
          </Field>
          <Field label="توکن افزونه‌ی SEO Master Bridge" hint={c?.hasSecret ? "ذخیره شده است — برای نگه‌داشتن آن خالی بگذارید." : undefined}>
            <input
              type="password"
              className={`${inputClass} ltr`}
              placeholder={c?.hasSecret ? "••••••••••••" : ""}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="new-password"
            />
          </Field>
        </div>
        <Field label="پراکسی (اختیاری)" hint="مثال: http://127.0.0.1:8080 یا socks5://127.0.0.1:1080">
          <input className={`${inputClass} ltr`} value={proxy} onChange={(e) => setProxy(e.target.value)} />
        </Field>

        <ErrorBox message={error} />
        {offline && <p className="text-xs text-amber-700">به نظر می‌رسد اینترنت قطع است. می‌توانید تلاش کنید (اگر سایت داخلی یا Proxy دارید ممکن است کار کند).</p>}

        <div className="flex flex-wrap gap-2">
          <Button onClick={run(save)} disabled={busy || !apiUrl}>
            {save.isPending ? "در حال ذخیره…" : "ذخیره‌ی اتصال"}
          </Button>
          <Button variant="secondary" onClick={run(test)} disabled={busy || !c}>
            {test.isPending ? "در حال تست…" : "تست اتصال"}
          </Button>
          <Button variant="secondary" onClick={run(sync)} disabled={busy || c?.status !== "CONNECTED"}>
            {sync.isPending ? "در حال دریافت…" : "دریافت دسته‌ها و برچسب‌ها"}
          </Button>
        </div>

        {testResult && (
          <div className="rounded-lg border border-emerald-200 bg-emerald-50 p-3 text-sm text-emerald-800">
            ✓ اتصال برقرار است — سایت: <b>{testResult.site || "—"}</b> | کاربر: <b>{testResult.user}</b> | آپلود رسانه:{" "}
            {testResult.mediaCapable ? "مجاز" : <Badge tone="amber">دسترسی آپلود ندارد</Badge>}
          </div>
        )}

        {c && (
          <dl className="grid grid-cols-2 gap-2 border-t border-slate-100 pt-3 text-xs text-slate-600">
            <div>آخرین تست: {faDate(c.lastTestedAt)}</div>
            <div>آخرین همگام‌سازی: {faDate(c.lastSyncedAt)}</div>
            {c.status === "FAILED" && c.lastError && <div className="col-span-2 text-red-600">آخرین خطا: {c.lastError}</div>}
          </dl>
        )}
      </Card>

      {(categories.data?.length ?? 0) > 0 && (
        <Card className="space-y-2">
          <h3 className="font-semibold">دسته‌ی پیش‌فرض مقالات</h3>
          <select className={inputClass} value={p.defaultCategoryWpId ?? ""} onChange={(e) => setDefaultCat.mutate(e.target.value)}>
            <option value="">دسته‌ی پیش‌فرض خود وردپرس</option>
            {categories.data!.map((t) => <option key={t.wpId} value={t.wpId}>{t.name}</option>)}
          </select>
          <p className="text-xs text-slate-500">مقاله‌هایی که دسته‌ی جداگانه ندارند در این دسته ساخته می‌شوند.</p>
        </Card>
      )}

      {syncResult && (
        <Card>
          <h3 className="mb-2 font-semibold">اطلاعات دریافت‌شده از وردپرس</h3>
          <div className="grid grid-cols-4 gap-3 text-center text-sm">
            <Stat label="دسته‌بندی" value={syncResult.categories} />
            <Stat label="برچسب" value={syncResult.tags} />
            <Stat label="نویسنده" value={syncResult.authors} />
            <Stat label="نوع نوشته" value={syncResult.postTypes} />
          </div>
        </Card>
      )}
    </div>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-lg bg-slate-50 p-3">
      <div className="text-xl font-bold text-brand-700">{num(value)}</div>
      <div className="text-xs text-slate-500">{label}</div>
    </div>
  );
}
