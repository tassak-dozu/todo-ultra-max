use rand::seq::SliceRandom;
use rand::Rng;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use uuid::Uuid;

use crate::db::now_ms;

const FREE_BURST_ACTIONS: usize = 3;
const WINDOW_MS: i64 = 60_000;
const RAPID_GAP_MS: i64 = 1_400;
const CHALLENGE_TTL_MS: i64 = 90_000;
const PASS_TTL_MS: i64 = 5 * 60 * 1000;
const PASS_CREDITS: u32 = 10;

const POOL: [(&str, &str); 12] = [
    ("🦀", "Blazingly Fast Crab"),
    ("🧠", "Quantum AI Brain"),
    ("🚀", "Turbo Rocket"),
    ("🔥", "Pure Aura Flame"),
    ("⚡", "Hyper Bolt"),
    ("🎰", "Degen Slot Machine"),
    ("🛸", "Cloud UFO"),
    ("💎", "Diamond Hands"),
    ("🐉", "Production Dragon"),
    ("🤖", "Friendly Overlord"),
    ("🍕", "3AM Deploy Pizza"),
    ("👑", "Kanban Crown"),
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptchaOption {
    pub id: String,
    pub emoji: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptchaChallengePublic {
    pub id: String,
    pub prompt: String,
    pub target_emoji: String,
    pub options: Vec<CaptchaOption>,
    pub expires_at: i64,
}

struct StoredChallenge {
    ip: String,
    answer_id: String,
    expires_at: i64,
}

struct HumanPass {
    remaining: u32,
    expires_at: i64,
}

struct CaptchaInner {
    recent_actions: HashMap<String, VecDeque<i64>>,
    passes: HashMap<String, HumanPass>,
    challenges: HashMap<String, StoredChallenge>,
    one_time_tokens: HashMap<String, (String, i64)>,
}

pub struct CaptchaGate {
    inner: Mutex<CaptchaInner>,
}

impl CaptchaGate {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(CaptchaInner {
                recent_actions: HashMap::new(),
                passes: HashMap::new(),
                challenges: HashMap::new(),
                one_time_tokens: HashMap::new(),
            }),
        }
    }

    /// Returns Ok(()) if this add/delete action is allowed without a popup right now,
    /// or Err(challenge) if the adaptive aura captcha should pop up.
    pub fn check_or_challenge(
        &self,
        ip: &str,
        captcha_token: Option<&str>,
    ) -> Result<(), CaptchaChallengePublic> {
        let now = now_ms();
        let mut inner = self.inner.lock().unwrap();

        // Purge expired tokens/challenges opportunistically
        if inner.challenges.len() > 256 {
            inner.challenges.retain(|_, c| c.expires_at > now);
        }
        if inner.one_time_tokens.len() > 256 {
            inner.one_time_tokens.retain(|_, (_, exp)| *exp > now);
        }

        // 1. Check explicit one-time verified captcha token
        if let Some(tok) = captcha_token {
            if let Some((tok_ip, exp)) = inner.one_time_tokens.remove(tok) {
                if exp > now && tok_ip == ip {
                    Self::record_action(&mut inner, ip, now);
                    return Ok(());
                }
            }
        }

        // 2. Check active verified human pass (grants 10 smooth actions for 5 min)
        if let Some(pass) = inner.passes.get_mut(ip) {
            if pass.expires_at > now && pass.remaining > 0 {
                pass.remaining -= 1;
                Self::record_action(&mut inner, ip, now);
                return Ok(());
            } else {
                inner.passes.remove(ip);
            }
        }

        // 3. Adaptive burst check: first 3 actions per minute (not faster than 1.4s apart) are frictionless
        let queue = inner.recent_actions.entry(ip.to_string()).or_default();
        while let Some(&front) = queue.front() {
            if now - front > WINDOW_MS {
                queue.pop_front();
            } else {
                break;
            }
        }

        let too_fast = queue.back().map(|&last| now - last < RAPID_GAP_MS).unwrap_or(false);
        let too_many = queue.len() >= FREE_BURST_ACTIONS;

        if !too_fast && !too_many {
            queue.push_back(now);
            return Ok(());
        }

        Err(Self::create_challenge_locked(&mut inner, ip, now))
    }

    fn record_action(inner: &mut CaptchaInner, ip: &str, now: i64) {
        let queue = inner.recent_actions.entry(ip.to_string()).or_default();
        queue.push_back(now);
        while queue.len() > 16 {
            queue.pop_front();
        }
    }

    pub fn issue_challenge(&self, ip: &str) -> CaptchaChallengePublic {
        let now = now_ms();
        let mut inner = self.inner.lock().unwrap();
        Self::create_challenge_locked(&mut inner, ip, now)
    }

    fn create_challenge_locked(
        inner: &mut CaptchaInner,
        ip: &str,
        now: i64,
    ) -> CaptchaChallengePublic {
        let mut rng = rand::thread_rng();
        let mut choices: Vec<(&str, &str)> = POOL.to_vec();
        choices.shuffle(&mut rng);
        choices.truncate(6);

        let target_idx = rng.gen_range(0..choices.len());
        let (target_emoji, target_label) = choices[target_idx];

        let mut answer_id = String::new();
        let mut options = Vec::with_capacity(choices.len());
        for (i, (emoji, label)) in choices.into_iter().enumerate() {
            let opt_id = Uuid::new_v4().to_string()[..8].to_string();
            if i == target_idx {
                answer_id = opt_id.clone();
            }
            options.push(CaptchaOption {
                id: opt_id,
                emoji: emoji.to_string(),
                label: label.to_string(),
            });
        }

        let id = Uuid::new_v4().to_string();
        let expires_at = now + CHALLENGE_TTL_MS;
        inner.challenges.insert(
            id.clone(),
            StoredChallenge {
                ip: ip.to_string(),
                answer_id,
                expires_at,
            },
        );

        CaptchaChallengePublic {
            id,
            prompt: format!("Tap the {target_label} to prove your aura"),
            target_emoji: target_emoji.to_string(),
            options,
            expires_at,
        }
    }

    pub fn verify(
        &self,
        ip: &str,
        challenge_id: &str,
        answer_id: &str,
    ) -> Result<(String, u32), String> {
        let now = now_ms();
        let mut inner = self.inner.lock().unwrap();

        let Some(ch) = inner.challenges.remove(challenge_id) else {
            return Err("Challenge expired or already used — try a fresh vibe check".to_string());
        };
        if ch.expires_at <= now {
            return Err("Challenge expired — try again".to_string());
        }
        if ch.ip != ip || ch.answer_id != answer_id {
            return Err("Wrong vibe! Robots exposed 🤖 — try again".to_string());
        }

        // Grant verified human pass + one-time token for immediate replay
        let token = Uuid::new_v4().to_string();
        inner
            .one_time_tokens
            .insert(token.clone(), (ip.to_string(), now + 60_000));
        inner.passes.insert(
            ip.to_string(),
            HumanPass {
                remaining: PASS_CREDITS,
                expires_at: now + PASS_TTL_MS,
            },
        );
        inner.recent_actions.remove(ip);

        Ok((token, PASS_CREDITS))
    }
}
