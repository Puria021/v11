import { useEffect, useState } from "react";
import { Navigate, NavLink, Route, Routes } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api/tauri";
import Dashboard from "./pages/Dashboard";
import Projects from "./pages/Projects";
import ProjectDetail from "./pages/ProjectDetail";
import Settings from "./pages/Settings";

import Campaigns from "./pages/Campaigns";
import Products from "./pages/Products";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import CampaignWizard from "./pages/CampaignWizard";
import { Badge } from "./components/ui";
import WhatsNew from "./components/WhatsNew";

const NAV = [
  { to: "/", label: "داشبورد", end: true },
  { to: "/projects", label: "پروژه‌ها" },
  { to: "/campaigns", label: "کمپین‌ها" },
  { to: "/products", label: "توضیحات محصولات" },
  { to: "/settings", label: "تنظیمات" },
];

export default function App() {
  const [online, setOnline] = useState<boolean | null>(null);
  const license = useQuery({ queryKey: ["license"], queryFn: api.getLicenseStatus });
  // The license tab exists only until the first activation.
  const nav = license.data?.activated === false ? [{ to: "/settings?tab=license", label: "لایسنس" }, ...NAV] : NAV;

  useEffect(() => {
    api.checkInternet().then(setOnline).catch(() => setOnline(null));
    const un = listen<{ online: boolean }>("connectivity:changed", (e) => setOnline(e.payload.online));
    return () => {
      un.then((f) => f());
    };
  }, []);

  // Windows notifications for long-running work (generation finished / errors).
  useEffect(() => {
    const un = listen<{ text: string }>("notify", async (e) => {
      try {
        let ok = await isPermissionGranted();
        if (!ok) ok = (await requestPermission()) === "granted";
        if (ok) sendNotification({ title: "مدیریت مقالات", body: e.payload.text });
      } catch { /* notifications are optional */ }
    });
    return () => { un.then((f) => f()); };
  }, []);

  return (
    <div className="flex h-full">
      <WhatsNew />
      <aside className="flex w-60 shrink-0 flex-col border-e border-slate-200 bg-white">
        <div className="px-5 py-5 text-lg font-bold text-brand-700">مدیریت مقالات</div>
        <nav className="flex-1 space-y-1 px-3">
          {nav.map((n) => (
            <NavLink
              key={n.to}
              to={n.to}
              end={n.end}
              className={({ isActive }) =>
                `block rounded-lg px-3 py-2 text-sm ${isActive ? "bg-brand-50 font-semibold text-brand-700" : "text-slate-600 hover:bg-slate-100"}`
              }
            >
              {n.label}
            </NavLink>
          ))}
        </nav>
        <div className="border-t border-slate-200 p-4 text-xs">
          اینترنت:{" "}
          {online === null ? <Badge tone="gray">نامشخص</Badge> : online ? <Badge tone="green">متصل ✓</Badge> : <Badge tone="amber">آفلاین</Badge>}
        </div>
      </aside>
      <main className="flex-1 overflow-auto p-8">
        <Routes>
          <Route path="/" element={<Dashboard online={online} />} />
          <Route path="/projects" element={<Projects />} />
          <Route path="/products" element={<Products />} />
          <Route path="/projects/:id" element={<ProjectDetail online={online} />} />
          <Route path="/campaigns" element={<Campaigns />} />
          <Route path="/campaigns/:id" element={<CampaignWizard online={online} />} />
          <Route path="/license" element={<Navigate to="/settings?tab=license" replace />} />
          {/* Templates, reports and updates now live in Settings. */}
          <Route path="/templates" element={<Navigate to="/settings?tab=templates" replace />} />
          {/* Provider settings are fixed now and no longer editable from the menu. */}
          <Route path="/providers" element={<Navigate to="/settings?tab=license" replace />} />
          <Route path="/image-providers" element={<Navigate to="/settings?tab=license" replace />} />
          <Route path="/keyword-providers" element={<Navigate to="/settings" replace />} />
          <Route path="/logs" element={<Navigate to="/settings?tab=logs" replace />} />
          <Route path="/settings" element={<Settings />} />
          <Route path="/updates" element={<Navigate to="/settings?tab=updates" replace />} />
        </Routes>
      </main>
    </div>
  );
}
