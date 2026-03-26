use axum_extra::extract::cookie::CookieJar;
use chrono::Utc;
use std::env;
use std::time::Duration;
use argon2::{
    password_hash::{
        PasswordHash, PasswordVerifier
    },
    Argon2
};
use crate::state::AppState;

pub fn init() {}

fn verify_hash(hash_str: &str, password_input: &str) -> bool {
    let parsed_hash = match PasswordHash::new(hash_str) {
        Ok(h) => h,
        Err(_) => return false,
    };

    Argon2::default()
        .verify_password(password_input.as_bytes(), &parsed_hash)
        .is_ok()
}

fn get_env_var(key: &str) -> String {
    env::var(key).unwrap_or_default()
}

pub fn verify_user(id: &str, pass: &str, key: &str) -> bool {
    let users = vec![
        (
            get_env_var("ADMIN_USER_1"),
            get_env_var("ADMIN_PASS_1"),
            get_env_var("ADMIN_KEY_1")
        ),
        (
            get_env_var("ADMIN_USER_2"),
            get_env_var("ADMIN_PASS_2"),
            get_env_var("ADMIN_KEY_2")
        ),
    ];

    for (env_user, env_pass_hash, env_key_hash) in users {
        if env_user.is_empty() {
            continue;
        }

        if id == env_user {
            let is_pass_valid = verify_hash(&env_pass_hash, pass);
            let is_key_valid = verify_hash(&env_key_hash, key);

            if is_pass_valid && is_key_valid {
                return true;
            }
        }
    }

    false
}

pub fn check_session(state: &AppState, jar: &CookieJar) -> bool {
    session_user_id(state, jar).is_some()
}

pub fn session_user_id(state: &AppState, jar: &CookieJar) -> Option<String> {
    let cookie = jar.get("session_id")?;
    let session_id = cookie.value();
    let now = Utc::now();
    let mut sessions = state.sessions.lock().unwrap();
    let user_id = sessions
        .get(session_id)
        .filter(|session| session.expires_at > now)
        .map(|session| session.user_id.clone());

    let removed = if user_id.is_none() {
        sessions.remove(session_id).is_some()
    } else {
        false
    };
    drop(sessions);

    if removed {
        state.persist_sessions();
    }

    user_id
}

pub fn spawn_session_cleaner(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            let removed = state.remove_expired_sessions();
            if removed > 0 {
                tracing::info!(removed, now = %Utc::now(), "expired admin sessions removed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        state::{AppState, Session},
        test_support::{dummy_pool, with_test_cwd},
    };
    use axum_extra::extract::cookie::{Cookie, CookieJar};
    use chrono::Duration as ChronoDuration;

    #[tokio::test]
    async fn session_user_id_returns_valid_user() {
        with_test_cwd("auth-valid-session", |_| {
            let state = AppState::new(dummy_pool());
            state.insert_session(
                "valid-session".to_string(),
                Session {
                    user_id: "admin".to_string(),
                    created_at: Utc::now(),
                    expires_at: Utc::now() + ChronoDuration::hours(1),
                },
            );

            let jar = CookieJar::new().add(Cookie::new("session_id", "valid-session"));
            assert_eq!(session_user_id(&state, &jar), Some("admin".to_string()));
            assert!(check_session(&state, &jar));
        });
    }

    #[tokio::test]
    async fn session_user_id_removes_expired_session() {
        with_test_cwd("auth-expired-session", |_| {
            let state = AppState::new(dummy_pool());
            state.insert_session(
                "expired-session".to_string(),
                Session {
                    user_id: "admin".to_string(),
                    created_at: Utc::now() - ChronoDuration::hours(2),
                    expires_at: Utc::now() - ChronoDuration::minutes(1),
                },
            );

            let jar = CookieJar::new().add(Cookie::new("session_id", "expired-session"));
            assert_eq!(session_user_id(&state, &jar), None);
            assert!(!state.sessions.lock().unwrap().contains_key("expired-session"));
        });
    }
}
