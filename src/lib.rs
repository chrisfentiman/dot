pub mod commands;
pub mod dotfiles;
pub mod runner;
pub mod secret;
pub mod ui;

/// Process-wide mutex for tests that mutate environment variables.
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub(crate) fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    match ENV_LOCK.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

#[cfg(test)]
pub(crate) struct EnvGuard {
    key: String,
    prev: Option<String>,
}

#[cfg(test)]
impl EnvGuard {
    /// Set an env var, returning a guard that restores the original value on drop.
    pub(crate) fn set(key: &str, val: &str) -> Self {
        let prev = std::env::var(key).ok();
        unsafe {
            std::env::set_var(key, val);
        }
        Self {
            key: key.to_string(),
            prev,
        }
    }
}

#[cfg(test)]
impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => unsafe {
                std::env::set_var(&self.key, v);
            },
            None => unsafe {
                std::env::remove_var(&self.key);
            },
        }
    }
}
