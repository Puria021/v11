# انتشار نسخه ویندوز و بروزرسانی خودکار

ریپازیتوری انتشار: https://github.com/Puria021/v4

1. در GitHub > Settings > Secrets and variables > Actions دو Secret تعریف کنید: `TAURI_SIGNING_PRIVATE_KEY` و `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (اگر کلید بدون گذرواژه است، مقدار خالی مجاز است).
2. در قسمت Variables یک متغیر `TAURI_UPDATER_PUBLIC_KEY` با متن کامل کلید عمومی ایجاد کنید. برای تولید جفت کلید از `npm run tauri signer generate -- -w ~/.tauri/myarticlemanager.key` استفاده کنید. کلید خصوصی را هرگز Commit نکنید.
3. سورس را به ریپازیتوری `Puria021/v4` منتقل کنید؛ workflow موجود `.github/workflows/build-windows.yml` را حفظ کنید و workflowهای انتشار قدیمی را غیرفعال کنید تا خروجی‌های تکراری تولید نشوند.
4. پس از Commit، Tag نسخه جدید را Push کنید: `git tag v4.0.0` سپس `git push origin v4.0.0`. Workflow نسخه داخل فایل تنظیمات را از Tag می‌گیرد.
5. پس از موفقیت Actions، در Releases باید یک NSIS setup.exe و فایل‌های فنی مربوط به آپدیت (از جمله .sig و latest.json) دیده شوند.

توجه: نصب خودکار از داخل برنامه نیازمند امضای معتبر و نصب نسخه پایه‌ای است که Updater آن فعال و با همان کلید عمومی ساخته شده باشد. این سورس هنوز در ویندوز Build/End-to-End تست نشده است.
