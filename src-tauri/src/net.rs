//! Central network helper: when the user's proxy setting is on, ALL app
//! traffic (ureq agents + webviews) uses it; otherwise connect directly.

const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

/// Normalized proxy URL when the proxy setting is on and valid.
pub fn proxy_url() -> Option<String> {
    let settings = crate::store::load_settings(&crate::store::Paths::detect());
    let raw = settings.auth_proxy()?;
    match crate::oauth::parse_proxy_url(raw) {
        Ok(u) => Some(u),
        Err(e) => {
            eprintln!("[zcs] proxy setting invalid, using direct: {e}");
            None
        }
    }
}

/// A ureq AgentBuilder with the proxy applied when enabled.
pub fn agent_builder(tag: &str) -> ureq::AgentBuilder {
    let b = ureq::AgentBuilder::new();
    match proxy_url() {
        Some(u) => match ureq::Proxy::new(&u) {
            Ok(p) => {
                eprintln!("[zcs] {tag}: proxy=on ({u})");
                b.proxy(p)
            }
            Err(e) => {
                eprintln!("[zcs] {tag}: proxy invalid ({e}), direct");
                b
            }
        },
        None => {
            eprintln!("[zcs] {tag}: proxy=off");
            b
        }
    }
}

/// Apply proxy (or --no-proxy-server when off) to a webview window builder.
pub fn webview<'a, R: tauri::Runtime, M: tauri::Manager<R>>(
    b: tauri::WebviewWindowBuilder<'a, R, M>,
    tag: &str,
) -> tauri::WebviewWindowBuilder<'a, R, M> {
    // Windows/WebView2: windows sharing the default profile must use identical
    // browser args, so the decision is fixed at startup (restart to apply changes).
    static STARTUP: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let cached = STARTUP.get_or_init(proxy_url).clone();
    match cached.and_then(|u| tauri::Url::parse(&u).ok()) {
        Some(u) => {
            eprintln!("[zcs] {tag} webview: proxy=on ({u})");
            b.additional_browser_args(BROWSER_ARGS).proxy_url(u)
        }
        None => {
            eprintln!("[zcs] {tag} webview: proxy=off");
            b.additional_browser_args(&format!("{BROWSER_ARGS} --no-proxy-server"))
        }
    }
}
