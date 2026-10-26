//! Minimal, versioned clipboard hand-off. No network calls or store writes.
use crate::i18n::tr;
use crate::store::{cred_plain, Account};
use crate::zcrypto;
use chrono::Local;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;

pub const FORMAT: &str = "zcode-switch-plus/magpie-account";

fn token(value: String) -> String {
    let text = value.trim();
    if text
        .get(..7)
        .is_some_and(|s| s.eq_ignore_ascii_case("bearer "))
    {
        text[7..].trim().to_string()
    } else {
        text.to_string()
    }
}

fn key_shape(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
}

fn id_site(id: &str) -> Option<&'static str> {
    if id.contains(":zai-") || id.contains(":zai:") || id == "builtin:zai" {
        Some("zai")
    } else if id.contains(":bigmodel-") || id.contains(":bigmodel:") || id == "builtin:bigmodel" {
        Some("bigmodel")
    } else {
        None
    }
}

fn config_site(id: &str, value: &Value) -> Option<&'static str> {
    let base = value
        .pointer("/options/baseURL")
        .and_then(Value::as_str)
        .unwrap_or("");
    let by_url = if base == "https://api.z.ai" || base.starts_with("https://api.z.ai/") {
        Some("zai")
    } else if base == "https://open.bigmodel.cn" || base.starts_with("https://open.bigmodel.cn/") {
        Some("bigmodel")
    } else {
        None
    };
    match (id_site(id), by_url) {
        (Some(a), Some(b)) if a != b => None,
        (a, b) => a.or(b),
    }
}

fn site_of(account: &Account, home: &Path) -> Result<&'static str, String> {
    if let Some(active) = cred_plain(&account.credentials, "oauth:active_provider", home) {
        return match active.trim() {
            "zai" => Ok("zai"),
            "bigmodel" => Ok("bigmodel"),
            _ => Err(tr("err.magpie.site_unknown")),
        };
    }
    let mut sites = BTreeSet::new();
    if let Some(providers) = account
        .config
        .as_ref()
        .and_then(|c| c.get("provider"))
        .and_then(Value::as_object)
    {
        for (id, value) in providers {
            if let Some(site) = config_site(id, value) {
                sites.insert(site);
            }
        }
    }
    if let Some(credentials) = account.credentials.as_object() {
        for id in credentials.keys() {
            if id.starts_with("account-provider:") && id.ends_with(":api-key") {
                if let Some(site) = id_site(id) {
                    sites.insert(site);
                }
            }
        }
    }
    if sites.len() == 1 {
        Ok(*sites.iter().next().unwrap())
    } else {
        Err(tr("err.magpie.site_unknown"))
    }
}

fn plan_key(account: &Account, site: &str, home: &Path) -> Option<String> {
    if let Some(providers) = account
        .config
        .as_ref()
        .and_then(|c| c.get("provider"))
        .and_then(Value::as_object)
    {
        let mut entries: Vec<_> = providers.iter().collect();
        entries.sort_by_key(|(_, v)| !v.get("enabled").and_then(Value::as_bool).unwrap_or(false));
        for (id, value) in entries {
            if id.contains("start") || config_site(id, value) != Some(site) {
                continue;
            }
            if !id.contains("coding-plan") {
                continue;
            }
            let Some(raw) = value.pointer("/options/apiKey").and_then(Value::as_str) else {
                continue;
            };
            let plain = if zcrypto::is_encrypted(raw) {
                zcrypto::decrypt_with_secret(raw, &zcrypto::default_secret(home)).ok()
            } else {
                Some(raw.to_string())
            };
            if let Some(key) = plain.map(token).filter(|k| key_shape(k)) {
                return Some(key);
            }
        }
    }
    if let Some(credentials) = account.credentials.as_object() {
        for id in credentials.keys() {
            if !id.starts_with("account-provider:")
                || !id.ends_with(":api-key")
                || !id.contains("coding-plan")
                || id.contains("start")
                || id_site(id) != Some(site)
            {
                continue;
            }
            if let Some(key) = cred_plain(&account.credentials, id, home)
                .map(token)
                .filter(|k| key_shape(k))
            {
                return Some(key);
            }
        }
    }
    None
}

pub fn clipboard(account: &Account, home: &Path) -> Result<String, String> {
    let site = site_of(account, home)?;
    let api_key = plan_key(account, site, home);
    let jwt = cred_plain(&account.credentials, "zcodejwttoken", home)
        .map(token)
        .filter(|t| {
            let parts: Vec<_> = t.split('.').collect();
            parts.len() == 3
                && parts.iter().all(|p| {
                    !p.is_empty()
                        && p.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                })
                && zcrypto::decode_jwt(t).is_some()
                && zcrypto::jwt_exp_ms(t)
                    .map_or(true, |exp| exp > Local::now().timestamp_millis() + 30_000)
        });
    if api_key.is_none() && jwt.is_none() {
        return Err(tr("err.magpie.no_credentials"));
    }
    let mut credentials = json!({});
    if let Some(key) = api_key {
        credentials["apiKey"] = json!(key);
    }
    if let Some(jwt) = jwt {
        credentials["jwt"] = json!(jwt);
    }
    let mut out = json!({
        "format": FORMAT,
        "version": 1,
        "site": site,
        "account": { "id": account.id, "name": account.name },
        "credentials": credentials,
    });
    if let Some(device) = account
        .virtual_device_mid
        .as_deref()
        .map(str::trim)
        .filter(|d| {
            !d.is_empty()
                && d.len() <= 128
                && d.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
        })
    {
        out["deviceId"] = json!(device);
    }
    serde_json::to_string(&out).map_err(|_| tr("err.magpie.serialize"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use sha2::{Digest, Sha256};

    fn account(credentials: Value, config: Option<Value>) -> Account {
        serde_json::from_value(json!({ "id":"source-account", "name":"工作账号", "created_at":"", "updated_at":"", "hash":"", "credentials":credentials, "config":config, "virtual_device_mid":"11111111-2222-4333-8444-555555555555" })).unwrap()
    }
    fn jwt(exp: i64) -> String {
        format!(
            "{}.{}.signature",
            URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256"}"#),
            URL_SAFE_NO_PAD.encode(json!({"exp":exp}).to_string())
        )
    }
    fn encrypted(value: &str, home: &Path) -> String {
        let key = Sha256::digest(zcrypto::default_secret(home).as_bytes());
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let nonce = [4u8; 12];
        let bytes = cipher
            .encrypt(Nonce::from_slice(&nonce), value.as_bytes())
            .unwrap();
        let (data, tag) = bytes.split_at(bytes.len() - 16);
        format!(
            "enc:v1:{}.{}.{}",
            URL_SAFE_NO_PAD.encode(nonce),
            URL_SAFE_NO_PAD.encode(tag),
            URL_SAFE_NO_PAD.encode(data)
        )
    }
    #[test]
    fn matches_the_shared_magpie_contract_fixture() {
        let expected: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/magpie-account.json")).unwrap();
        let a = account(
            json!({"oauth:active_provider":expected["site"],"zcodejwttoken":expected["credentials"]["jwt"],"account-provider:coding-plan:account:zai-individual-coding-plan:account:u:api-key":expected["credentials"]["apiKey"]}),
            None,
        );
        let actual: Value =
            serde_json::from_str(&clipboard(&a, Path::new("/test-home")).unwrap()).unwrap();
        assert_eq!(actual, expected);
    }
    #[test]
    fn copies_both_credentials_with_account_site_and_device() {
        let j = jwt(Local::now().timestamp() + 3600);
        let a = account(
            json!({"oauth:active_provider":"zai", "zcodejwttoken":j}),
            Some(
                json!({"provider":{"builtin:zai-coding-plan":{"enabled":true,"options":{"apiKey":"paid.secret","baseURL":"https://api.z.ai/api/anthropic"}}}}),
            ),
        );
        let text = clipboard(&a, Path::new("/test-home")).unwrap();
        assert!(!text.contains('\n'));
        let out: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(out["format"], FORMAT);
        assert_eq!(out["version"], 1);
        assert_eq!(out["site"], "zai");
        assert_eq!(out["account"]["name"], "工作账号");
        assert_eq!(out["credentials"]["apiKey"], "paid.secret");
        assert_eq!(out["credentials"]["jwt"], j);
        assert_eq!(out["deviceId"], "11111111-2222-4333-8444-555555555555");
        assert!(out.get("config").is_none());
    }
    #[test]
    fn decrypts_new_credentials_and_excludes_other_site_and_business_token() {
        let home = Path::new("/test-home");
        let a = account(
            json!({"oauth:active_provider":encrypted("bigmodel",home),"oauth:bigmodel:access_token":"do-not-export", "account-provider:coding-plan:account:zai-individual-coding-plan:account:u:api-key":"wrong.secret", "account-provider:coding-plan:account:bigmodel-individual-coding-plan:account:u:api-key":encrypted("right.secret",home)}),
            None,
        );
        let text = clipboard(&a, home).unwrap();
        let out: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(out["site"], "bigmodel");
        assert_eq!(out["credentials"]["apiKey"], "right.secret");
        assert!(!text.contains("do-not-export") && !text.contains("wrong.secret"));
    }
    #[test]
    fn expired_jwt_is_omitted_while_paid_key_is_kept() {
        let a = account(
            json!({"oauth:active_provider":"zai","zcodejwttoken":jwt(1),"account-provider:coding-plan:account:zai-individual-coding-plan:account:u:api-key":"paid.secret"}),
            None,
        );
        let out: Value =
            serde_json::from_str(&clipboard(&a, Path::new("/test-home")).unwrap()).unwrap();
        assert_eq!(out["credentials"]["apiKey"], "paid.secret");
        assert!(out["credentials"].get("jwt").is_none());
    }
    #[test]
    fn jwt_only_account_copies_without_paid_key() {
        let home = Path::new("/test-home");
        let a = account(
            json!({"oauth:active_provider":"zai","zcodejwttoken":encrypted(&jwt(Local::now().timestamp()+3600),home)}),
            None,
        );
        let out: Value = serde_json::from_str(&clipboard(&a, home).unwrap()).unwrap();
        assert!(out["credentials"].get("apiKey").is_none());
        assert!(out["credentials"]["jwt"].is_string());
    }
    #[test]
    fn refuses_ambiguous_site_or_expired_session() {
        let a = account(json!({"zcodejwttoken":jwt(1)}), None);
        assert!(clipboard(&a, Path::new("/test-home")).is_err());
        let a = account(
            json!({"oauth:active_provider":"zai","zcodejwttoken":jwt(1)}),
            None,
        );
        assert!(clipboard(&a, Path::new("/test-home")).is_err());
    }
    #[test]
    fn start_jwt_is_not_mistaken_for_a_paid_api_key() {
        let j = jwt(Local::now().timestamp() + 3600);
        let a = account(
            json!({"oauth:active_provider":"zai","zcodejwttoken":j}),
            Some(
                json!({"provider":{"builtin:zai-start-plan":{"options":{"apiKey":j,"baseURL":"https://zcode.z.ai/api/v1/zcode-plan/anthropic"}}}}),
            ),
        );
        let out: Value =
            serde_json::from_str(&clipboard(&a, Path::new("/test-home")).unwrap()).unwrap();
        assert!(out["credentials"].get("apiKey").is_none());
    }
}
