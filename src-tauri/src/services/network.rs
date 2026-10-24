//! Application-scoped networking. Does not change Windows/system DNS or proxy settings.
//! Hosts in dnsOverrides are resolved to the specified IP by reqwest for all HTTP clients.
use crate::error::{AppError, AppResult};
use crate::providers::{ProviderError, ProviderErrorKind};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use std::sync::{OnceLock, RwLock};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSettings {
    pub proxy_url: String,
    /// newline separated domain=IP[:PORT] mappings. Default port is HTTPS 443.
    pub dns_overrides: String,
}

fn global() -> &'static RwLock<NetworkSettings> {
    static CONFIG: OnceLock<RwLock<NetworkSettings>> = OnceLock::new();
    CONFIG.get_or_init(|| RwLock::new(NetworkSettings::default()))
}

fn validate(input: &NetworkSettings) -> AppResult<Vec<(String, SocketAddr)>> {
    if !input.proxy_url.trim().is_empty() {
        let u = url::Url::parse(input.proxy_url.trim()).map_err(|_| AppError::validation("آدرس Proxy معتبر نیست."))?;
        if !matches!(u.scheme(), "http" | "https" | "socks5" | "socks5h") || u.host_str().is_none() {
            return Err(AppError::validation("پروکسی باید با http://، https:// یا socks5:// شروع شود."));
        }
    }
    let mut overrides = Vec::new();
    for line in input.dns_overrides.lines().map(str::trim).filter(|s| !s.is_empty() && !s.starts_with('#')) {
        let (host, ip) = line.split_once('=').ok_or_else(|| AppError::validation("فرمت DNS: دامنه=IP (هر خط یک مورد)"))?;
        let host = host.trim().to_ascii_lowercase();
        if host.is_empty() || host.len() > 253 || host.starts_with('.') || host.contains("..") ||
            !host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') {
            return Err(AppError::validation("نام دامنه در DNS Override نامعتبر است."));
        }
        let raw = ip.trim();
        // Hostname overrides typically target HTTPS (443). For HTTP or custom ports,
        // accept an explicit SocketAddr, including [IPv6]:port.
        let address: SocketAddr = match raw.parse::<SocketAddr>() {
            Ok(addr) => addr,
            Err(_) => SocketAddr::new(raw.parse::<IpAddr>().map_err(|_| AppError::validation("مقدار DNS باید IP یا IP:PORT باشد."))?, 443),
        };
        if address.port() == 0 { return Err(AppError::validation("پورت DNS نباید صفر باشد.")); }
        overrides.push((host, address));
    }
    if overrides.len() > 100 { return Err(AppError::validation("حداکثر ۱۰۰ دامنه در DNS Override مجاز است.")); }
    Ok(overrides)
}

pub async fn initialize(pool: &sqlx::SqlitePool) -> AppResult<()> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value_json FROM settings WHERE key='network.settings'")
        .fetch_optional(pool).await?;
    let config = row.and_then(|r| serde_json::from_str::<NetworkSettings>(&r.0).ok()).unwrap_or_default();
    if validate(&config).is_ok() { *global().write().unwrap_or_else(|e| e.into_inner()) = config; }
    Ok(())
}

pub async fn get(_state: &AppState) -> NetworkSettings {
    global().read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub async fn save(state: &AppState, settings: NetworkSettings) -> AppResult<NetworkSettings> {
    validate(&settings)?;
    let serialized = serde_json::to_string(&settings).map_err(|_| AppError::internal("خطا در ذخیره تنظیمات شبکه"))?;
    sqlx::query("INSERT INTO settings (key, value_json) VALUES ('network.settings', ?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json")
        .bind(serialized).execute(&state.db).await?;
    *global().write().unwrap_or_else(|e| e.into_inner()) = settings.clone();
    Ok(settings)
}

/// Reusable HTTP client builder hook for WordPress, AI and image providers.
/// A provider-specific proxy, if set, wins over the application-wide proxy.
pub fn apply_http(mut b: reqwest::ClientBuilder, provider_proxy: Option<&str>) -> Result<reqwest::ClientBuilder, ProviderError> {
    let config = global().read().unwrap_or_else(|e| e.into_inner()).clone();
    let proxy = provider_proxy.filter(|s| !s.trim().is_empty()).unwrap_or(config.proxy_url.trim());
    if !proxy.is_empty() {
        b = b.proxy(reqwest::Proxy::all(proxy)
            .map_err(|_| ProviderError::new(ProviderErrorKind::Network, "آدرس Proxy معتبر نیست."))?);
    }
    let overrides = validate(&config).map_err(|_| ProviderError::new(ProviderErrorKind::Network, "DNS Override نامعتبر است."))?;
    for (host, address) in overrides {
        b = b.resolve(&host, address);
    }
    Ok(b)
}
