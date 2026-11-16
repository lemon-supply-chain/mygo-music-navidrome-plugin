// Accounts persisted next to the executable as `auth.json`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::random;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub url: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub last_used_at: u64,
}

/// What the page is allowed to see. Passwords never leave the server.
#[derive(Clone, Debug, Serialize)]
pub struct AccountView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    pub label: String,
    pub last_used_at: u64,
}

impl Account {
    pub fn label(&self) -> String {
        let name = self.name.trim();
        if name.is_empty() {
            self.url.clone()
        } else {
            name.to_string()
        }
    }

    pub fn view(&self) -> AccountView {
        AccountView {
            id: self.id.clone(),
            name: self.name.clone(),
            url: self.url.clone(),
            username: self.username.clone(),
            label: self.label(),
            last_used_at: self.last_used_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Store {
    #[serde(default = "version")]
    pub version: u32,
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub current: Option<String>,
}

fn version() -> u32 {
    1
}

impl Default for Store {
    fn default() -> Self {
        Self { version: version(), accounts: Vec::new(), current: None }
    }
}

impl Store {
    pub fn load(path: &Path) -> Store {
        let Ok(raw) = fs::read_to_string(path) else {
            return Store::default();
        };
        match serde_json::from_str(&raw) {
            Ok(store) => store,
            Err(error) => {
                eprintln!("auth file {} is unreadable: {error}", path.display());
                Store::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let body = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, body)?;
        restrict(&temp)?;
        fs::rename(&temp, path)?;
        Ok(())
    }

    pub fn current_account(&self) -> Option<&Account> {
        let id = self.current.as_ref()?;
        self.accounts.iter().find(|account| &account.id == id)
    }

    /// Same server and username replaces the stored password instead of adding a row.
    pub fn upsert(&mut self, name: String, url: String, username: String, password: String) -> Account {
        let now = now();
        if let Some(existing) =
            self.accounts.iter_mut().find(|account| account.url == url && account.username == username)
        {
            // Signing in again without typing a name keeps the stored one.
            if !name.trim().is_empty() {
                existing.name = name;
            }
            existing.password = password;
            existing.last_used_at = now;
            let account = existing.clone();
            self.current = Some(account.id.clone());
            return account;
        }

        let mut id = random_id();
        while self.accounts.iter().any(|account| account.id == id) {
            id = random_id();
        }
        let account = Account { id: id.clone(), name, url, username, password, created_at: now, last_used_at: now };
        self.accounts.push(account.clone());
        self.current = Some(id);
        account
    }

    pub fn switch(&mut self, id: &str) -> Option<Account> {
        let now = now();
        let account = self.accounts.iter_mut().find(|account| account.id == id)?;
        account.last_used_at = now;
        let account = account.clone();
        self.current = Some(account.id.clone());
        Some(account)
    }

    /// Rewrites a saved account in place, keeping its id and its place in line.
    pub fn update(
        &mut self,
        id: &str,
        name: String,
        url: String,
        username: String,
        password: String,
    ) -> Option<Account> {
        let now = now();
        let account = self.accounts.iter_mut().find(|account| account.id == id)?;
        account.name = name;
        account.url = url;
        account.username = username;
        account.password = password;
        account.last_used_at = now;
        Some(account.clone())
    }

    /// Whether another account already answers to this server and user.
    pub fn clashes(&self, id: &str, url: &str, username: &str) -> bool {
        self.accounts.iter().any(|account| account.id != id && account.url == url && account.username == username)
    }

    pub fn find(&self, id: &str) -> Option<Account> {
        self.accounts.iter().find(|account| account.id == id).cloned()
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.accounts.len();
        self.accounts.retain(|account| account.id != id);
        if self.accounts.len() == before {
            return false;
        }
        if self.current.as_deref() == Some(id) {
            self.current = self.accounts.first().map(|account| account.id.clone());
        }
        true
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

fn random_id() -> String {
    random::pick(b"0123456789abcdefghijklmnopqrstuvwxyz", 12)
}

fn restrict(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub fn default_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_in(store: &mut Store, name: &str, url: &str, user: &str) -> Account {
        store.upsert(name.into(), url.into(), user.into(), "password".into())
    }

    #[test]
    fn the_same_account_replaces_instead_of_piling_up() {
        let mut store = Store::default();
        let first = signed_in(&mut store, "Living room", "http://a", "alice");
        let again = signed_in(&mut store, "", "http://a", "alice");
        assert_eq!(store.accounts.len(), 1);
        assert_eq!(first.id, again.id);
        assert_eq!(again.name, "Living room");
        assert_eq!(store.current.as_deref(), Some(first.id.as_str()));
    }

    #[test]
    fn other_users_and_servers_are_kept_apart() {
        let mut store = Store::default();
        signed_in(&mut store, "", "http://a", "alice");
        signed_in(&mut store, "", "http://a", "bob");
        signed_in(&mut store, "", "http://b", "alice");
        assert_eq!(store.accounts.len(), 3);
    }

    #[test]
    fn removing_the_current_account_falls_back() {
        let mut store = Store::default();
        signed_in(&mut store, "", "http://a", "alice");
        let second = signed_in(&mut store, "", "http://b", "bob");
        assert_eq!(store.current.as_deref(), Some(second.id.as_str()));
        assert!(store.remove(&second.id));
        assert!(store.current_account().is_some());
        assert!(!store.remove(&second.id));
    }

    #[test]
    fn an_account_can_be_rewritten_in_place() {
        let mut store = Store::default();
        let first = signed_in(&mut store, "Living room", "http://a", "alice");
        signed_in(&mut store, "", "http://b", "bob");

        let changed =
            store.update(&first.id, "Den".into(), "http://c".into(), "carol".into(), "new".into()).expect("updated");
        assert_eq!(store.accounts.len(), 2);
        assert_eq!(changed.id, first.id);
        assert_eq!(changed.name, "Den");
        assert_eq!(changed.url, "http://c");
        assert_eq!(changed.username, "carol");
        assert_eq!(changed.password, "new");
        assert!(store.update("nobody", "".into(), "".into(), "".into(), "".into()).is_none());
    }

    #[test]
    fn the_same_server_and_user_cannot_be_saved_twice() {
        let mut store = Store::default();
        let first = signed_in(&mut store, "", "http://a", "alice");
        let second = signed_in(&mut store, "", "http://b", "bob");

        assert!(store.clashes(&first.id, "http://b", "bob"));
        assert!(store.clashes(&second.id, "http://a", "alice"));
        assert!(!store.clashes(&first.id, "http://a", "alice"));
        assert!(!store.clashes(&second.id, "http://b", "bob"));
        assert!(!store.clashes(&second.id, "http://a", "bob"));
    }

    #[test]
    fn label_prefers_the_name() {
        let mut store = Store::default();
        let named = signed_in(&mut store, "Living room", "http://a", "alice");
        let unnamed = signed_in(&mut store, "  ", "http://b", "bob");
        assert_eq!(named.label(), "Living room");
        assert_eq!(unnamed.label(), "http://b");
    }
}
