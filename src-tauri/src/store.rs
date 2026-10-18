//! Persistência local das TVs conhecidas (IP, MAC, nome, modelo e token de
//! pareamento). O arquivo vive na pasta de dados do app (`devices.json`).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct StoredDevice {
    pub ip: String,
    #[serde(default)]
    pub mac: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct Db {
    #[serde(default)]
    devices: Vec<StoredDevice>,
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("devices.json"))
}

fn load(app: &AppHandle) -> Db {
    path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn persist(app: &AppHandle, db: &Db) {
    if let Some(p) = path(app) {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(db) {
            let _ = std::fs::write(p, json);
        }
    }
}

fn matches(d: &StoredDevice, ip: &str, mac: Option<&str>) -> bool {
    d.ip == ip || (mac.is_some() && d.mac.as_deref() == mac)
}

pub fn all(app: &AppHandle) -> Vec<StoredDevice> {
    load(app).devices
}

pub fn find(app: &AppHandle, ip: &str) -> Option<StoredDevice> {
    load(app).devices.into_iter().find(|d| matches(d, ip, None))
}

pub fn upsert(app: &AppHandle, dev: StoredDevice) {
    let mut db = load(app);
    if let Some(existing) = db
        .devices
        .iter_mut()
        .find(|d| matches(d, &dev.ip, dev.mac.as_deref()))
    {
        if dev.mac.is_some() {
            existing.mac = dev.mac.clone();
        }
        if dev.name.is_some() {
            existing.name = dev.name.clone();
        }
        if dev.model.is_some() {
            existing.model = dev.model.clone();
        }
        if dev.token.is_some() {
            existing.token = dev.token.clone();
        }
        existing.ip = dev.ip.clone();
    } else {
        db.devices.push(dev);
    }
    persist(app, &db);
}

pub fn remove(app: &AppHandle, ip: &str) {
    let mut db = load(app);
    db.devices.retain(|d| d.ip != ip);
    persist(app, &db);
}

pub fn clear_token(app: &AppHandle, ip: &str) {
    let mut db = load(app);
    for d in db.devices.iter_mut() {
        if d.ip == ip {
            d.token = None;
        }
    }
    persist(app, &db);
}
