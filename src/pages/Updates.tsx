import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";
import { Button, Card } from "../components/ui";
export default function Updates() {
 const [current,setCurrent]=useState("…"); const [update,setUpdate]=useState<Update|null>(null);
 const [message,setMessage]=useState("برای بررسی بروزرسانی کلیک کنید."); const [busy,setBusy]=useState(false);
 useEffect(()=>{getVersion().then(setCurrent).catch(()=>{}); void checkNow();},[]);
 async function checkNow(){setBusy(true);try{const u=await check();setUpdate(u);setMessage(u?`نسخه ${u.version} آماده نصب است. ${u.body||""}`:"شما آخرین نسخه را دارید.");}catch(e){setMessage(`بررسی ناموفق: ${String(e)}`)}finally{setBusy(false)}}
 async function install(){if(!update)return;setBusy(true);try{await update.downloadAndInstall(e=>{if(e.event==="Started")setMessage("دریافت بروزرسانی شروع شد…");if(e.event==="Finished")setMessage("دانلود تکمیل شد؛ در حال نصب…");});await relaunch();}catch(e){setMessage(`نصب ناموفق: ${String(e)}`);setBusy(false)}}
 return <div className="max-w-2xl space-y-5"><h1 className="text-2xl font-bold">بروزرسانی نرم‌افزار</h1><Card className="space-y-4"><p>نسخه نصب‌شده: {current}</p><p className="text-sm">{message}</p><div className="flex gap-2"><Button disabled={busy} onClick={checkNow}>بررسی نسخه جدید</Button>{update&&<Button disabled={busy} onClick={install}>دانلود و نصب</Button>}</div></Card></div>
}
