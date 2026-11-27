use reqwest::Client;
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use uuid::Uuid;

use crate::db::now_ms;

const FREE_BURST_ACTIONS: usize = 3;
const WINDOW_MS: i64 = 60_000;
const RAPID_GAP_MS: i64 = 1_400;
const PASS_TTL_MS: i64 = 5 * 60 * 1000;
const PASS_CREDITS: u32 = 10;
const TURNSTILE_VERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";

struct HumanPass {
    remaining: u32,
    expires_at: i64,
}

struct GateInner {
    recent_actions: HashMap<String, VecDeque<i64>>,
    passes: HashMap<String, HumanPass>,
    one_time_tokens: HashMap<String, (String, i64)>,
}

#[derive(Deserialize)]
struct SiteVerifyResponse {
    success: bool,
    #[serde(rename = "error-codes", default)]
    error_codes: Vec<String>,
}

pub struct TurnstileGate {
    secret_key: Option<String>,
    pub site_key: Option<String>,
    http: Client,
    inner: Mutex<GateInner>,
}

impl TurnstileGate {
    pub fn new(site_key: String, secret_key: String) -> Self {
        let sk = secret_key.trim().to_string();
        let pk = site_key.trim().to_string();
        Self {
            secret_key: if sk.is_empty() { None } else { Some(sk) },
            site_key: if pk.is_empty() { None } else { Some(pk) },
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(8))
                .build()
                .expect("Failed to build HTTP client"),
            inner: Mutex::new(GateInner {
                recent_actions: HashMap::new(),
                passes: HashMap::new(),
                one_time_tokens: HashMap::new(),
            }),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.secret_key.is_some()
    }

    /// Returns Ok(()) if action is permitted immediately without Turnstile,
    /// or Err(()) if the caller must complete Cloudflare Turnstile.
    pub fn check_or_require(
        &self,
        ip: &str,
        one_time_token: Option<&str>,
    ) -> Result<(), ()> {
        if !self.is_enabled() {
            return Ok(());
        }

        let now = now_ms();
        let mut inner = self.inner.lock().unwrap();

        if inner.one_time_tokens.len() > 256 {
            inner.one_time_tokens.retain(|_, (_, exp)| *exp > now);
        }

        // 1. Check one-time server token minted immediately after Turnstile verification
        if let Some(tok) = one_time_token {
            if let Some((tok_ip, exp)) = inner.one_time_tokens.remove(tok) {
                if exp > now && tok_ip == ip {
                    Self::record_action(&mut inner, ip, now);
                    return Ok(());
                }
            }
        }

        // 2. Check active verified human pass (10 smooth actions for 5 min)
        if let Some(pass) = inner.passes.get_mut(ip) {
            if pass.expires_at > now && pass.remaining > 0 {
                pass.remaining -= 1;
                Self::record_action(&mut inner, ip, now);
                return Ok(());
            } else {
                inner.passes.remove(ip);
            }
        }

        // 3. Adaptive burst check: first 3 adds/deletes per minute (>= 1.4s apart) require no captcha
        let queue = inner.recent_actions.entry(ip.to_string()).or_default();
        while let Some(&front) = queue.front() {
            if now - front > WINDOW_MS {
                queue.pop_front();
            } else {
                break;
            }
        }

        let too_fast = queue
            .back()
            .map(|&last| now - last < RAPID_GAP_MS)
            .unwrap_or(false);
        let too_many = queue.len() >= FREE_BURST_ACTIONS;

        if !too_fast && !too_many {
            queue.push_back(now);
            return Ok(());
        }

        Err(())
    }

    fn record_action(inner: &mut GateInner, ip: &str, now: i64) {
        let queue = inner.recent_actions.entry(ip.to_string()).or_default();
        queue.push_back(now);
        while queue.len() > 16 {
            queue.pop_front();
        }
    }

    /// Verifies a real Cloudflare Turnstile client token via siteverify and grants a 10-move pass.
    pub async fn verify_turnstile(&self, ip: &str, cf_token: &str) -> Result<(String, u32), String> {
        let Some(ref secret) = self.secret_key else {
            return Err("Turnstile is not configured on this server".to_string());
        };
        let token_trimmed = cf_token.trim();
        if token_trimmed.is_empty() {
            return Err("Missing Cloudflare Turnstile response token".to_string());
        }

        let mut form = vec![
            ("secret", secret.as_str()),
            ("response", token_trimmed),
        ];
        if ip != "local" && !ip.is_empty() {
            form.push(("remoteip", ip));
        }

        let resp = self
            .http
            .post(TURNSTILE_VERIFY_URL)
            .form(&form)
            .send()
            .await
            .map_err(|e| format!("Turnstile verification network error: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("Turnstile siteverify HTTP {}", resp.status()));
        }

        let parsed: SiteVerifyResponse = resp
            .json()
            .await
            .map_err(|e| format!("Invalid Turnstile response: {e}"))?;

        if !parsed.success {
            let codes = if parsed.error_codes.is_empty() {
                "verification-failed".to_string()
            } else {
                parsed.error_codes.join(", ")
            };
            return Err(format!("Cloudflare Turnstile rejected token ({codes})"));
        }

        let now = now_ms();
        let one_time = Uuid::new_v4().to_string();
        let mut inner = self.inner.lock().unwrap();
        inner
            .one_time_tokens
            .insert(one_time.clone(), (ip.to_string(), now + 60_000));
        inner.passes.insert(
            ip.to_string(),
            HumanPass {
                remaining: PASS_CREDITS,
                expires_at: now + PASS_TTL_MS,
            },
        );
        inner.recent_actions.remove(ip);

        Ok((one_time, PASS_CREDITS))
    }
}
