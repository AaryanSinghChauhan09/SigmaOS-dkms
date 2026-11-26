//! SigmaOS dkms — kernel module (DKMS) manager.
//!
//! Tracks third-party kernel modules, their versions, and build status per
//! kernel release. JSON-backed state; build/install delegated to dkms(8).

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Errors produced by this component.
#[derive(Debug, thiserror::Error)]
pub enum DkmsError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("module not found: {0}")]
    NotFound(String),
    #[error("validation error: {0}")]
    Validation(String),
}

/// Build status of a module for a kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModuleStatus {
    /// Not built for this kernel.
    None,
    /// Built successfully.
    Built,
    /// Installed.
    Installed,
    /// Build failed.
    Failed,
}

/// A DKMS-managed module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelModule {
    pub name: String,
    pub version: String,
    /// kernel release -> status
    #[serde(default)]
    pub kernels: BTreeMap<String, ModuleStatus>,
}

/// DKMS state database.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DkmsDb {
    #[serde(default)]
    pub modules: BTreeMap<String, KernelModule>,
}

impl DkmsDb {
    /// Register a module.
    pub fn add_module(&mut self, name: &str, version: &str) -> Result<(), DkmsError> {
        if name.is_empty() || version.is_empty() {
            return Err(DkmsError::Validation("name/version required".into()));
        }
        self.modules.insert(
            name.to_string(),
            KernelModule {
                name: name.into(),
                version: version.into(),
                kernels: BTreeMap::new(),
            },
        );
        Ok(())
    }

    /// Remove a module.
    pub fn remove_module(&mut self, name: &str) -> Result<(), DkmsError> {
        if self.modules.remove(name).is_none() {
            return Err(DkmsError::NotFound(name.into()));
        }
        Ok(())
    }

    /// Set status for a module on a kernel release.
    pub fn set_status(
        &mut self,
        name: &str,
        kernel: &str,
        status: ModuleStatus,
    ) -> Result<(), DkmsError> {
        let m = self
            .modules
            .get_mut(name)
            .ok_or_else(|| DkmsError::NotFound(name.into()))?;
        m.kernels.insert(kernel.to_string(), status);
        Ok(())
    }

    /// Modules not built for a given kernel.
    pub fn missing_for_kernel(&self, kernel: &str) -> Vec<&KernelModule> {
        self.modules
            .values()
            .filter(|m| {
                !matches!(
                    m.kernels.get(kernel),
                    Some(ModuleStatus::Built) | Some(ModuleStatus::Installed)
                )
            })
            .collect()
    }

    /// List all modules.
    pub fn list(&self) -> Vec<&KernelModule> {
        self.modules.values().collect()
    }

    /// Save to JSON.
    pub fn save(&self, path: &Path) -> Result<(), DkmsError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Load from JSON.
    pub fn load(path: &Path) -> Result<Self, DkmsError> {
        let text = std::fs::read_to_string(path)
            .map_err(|_| DkmsError::NotFound(path.display().to_string()))?;
        Ok(serde_json::from_str(&text)?)
    }
}

/// Default state path.
pub fn default_db_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("org", "sigmaos", "sigmaos")
        .map(|p| p.data_dir().join("dkms").join("db.json"))
}

/// Current running kernel release.
pub fn current_kernel() -> String {
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_remove_and_status() {
        let mut db = DkmsDb::default();
        db.add_module("vboxdrv", "6.1.0").unwrap();
        assert!(db.list().len() == 1);
        assert!(matches!(
            db.add_module("", "1.0"),
            Err(DkmsError::Validation(_))
        ));
        db.set_status("vboxdrv", "6.8.0", ModuleStatus::Built)
            .unwrap();
        db.set_status("vboxdrv", "6.8.0", ModuleStatus::Installed)
            .unwrap();
        assert!(matches!(
            db.set_status("ghost", "6.8.0", ModuleStatus::Built),
            Err(DkmsError::NotFound(_))
        ));
        db.remove_module("vboxdrv").unwrap();
        assert!(matches!(
            db.remove_module("vboxdrv"),
            Err(DkmsError::NotFound(_))
        ));
    }

    #[test]
    fn missing_for_kernel() {
        let mut db = DkmsDb::default();
        db.add_module("m1", "1.0").unwrap();
        db.add_module("m2", "2.0").unwrap();
        db.set_status("m1", "6.8.0", ModuleStatus::Installed)
            .unwrap();
        db.set_status("m2", "6.8.0", ModuleStatus::Failed).unwrap();
        // m1 is installed for 6.8.0; m2 failed -> still missing.
        let missing = db.missing_for_kernel("6.8.0");
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].name, "m2");
        // Unknown kernel: both missing.
        assert_eq!(db.missing_for_kernel("6.9.0").len(), 2);
        // Built also counts as present.
        db.set_status("m2", "6.8.0", ModuleStatus::Built).unwrap();
        assert!(db.missing_for_kernel("6.8.0").is_empty());
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.json");
        let mut db = DkmsDb::default();
        db.add_module("rtl88xxau", "1.0").unwrap();
        db.set_status("rtl88xxau", "6.8.0-arch1-1", ModuleStatus::Built)
            .unwrap();
        db.save(&path).unwrap();
        let loaded = DkmsDb::load(&path).unwrap();
        assert_eq!(loaded.modules.len(), 1);
        assert_eq!(
            loaded.modules["rtl88xxau"].kernels["6.8.0-arch1-1"],
            ModuleStatus::Built
        );
        assert!(DkmsDb::load(&dir.path().join("missing.json")).is_err());
    }
}
