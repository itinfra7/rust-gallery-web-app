use sqlx::{postgres::PgPoolOptions, PgPool};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn dummy_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgres://DBUSERNAME:DBUSERPASSWORD@DBHOST/postgres")
        .expect("dummy lazy pool should parse")
}

pub fn with_test_cwd<T>(name: &str, f: impl FnOnce(&Path) -> T) -> T {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _lock = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let original_dir = env::current_dir().expect("current dir");
    let temp_dir = unique_temp_dir(name);
    fs::create_dir_all(&temp_dir).expect("create temp dir");

    struct RestoreGuard {
        original_dir: PathBuf,
        temp_dir: PathBuf,
    }

    impl Drop for RestoreGuard {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.original_dir);
            let _ = fs::remove_dir_all(&self.temp_dir);
        }
    }

    env::set_current_dir(&temp_dir).expect("set temp dir");
    let _guard = RestoreGuard {
        original_dir,
        temp_dir: temp_dir.clone(),
    };

    f(&temp_dir)
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    env::temp_dir().join(format!("gallery-app-{}-{}", name, nanos))
}
