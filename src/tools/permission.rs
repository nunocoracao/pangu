//! Permission management for tool execution
//!
//! Handles per-tool, per-path permissions stored in `.pangu/permissions.json`
//! within the project directory.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use super::ToolContext;

/// Permission decision made by the user
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    /// Allow this specific request once (not persisted)
    AllowOnce,
    /// Always allow this path/pattern for this tool
    AlwaysAllow,
    /// Deny this request
    Deny,
}

/// Result of permission check
#[derive(Debug, Clone)]
pub enum PermissionCheckResult {
    /// Permission granted (either no permission needed or already allowed)
    Allowed,
    /// Permission explicitly denied
    Denied,
    /// Needs user permission - show prompt
    NeedsPermission {
        tool_name: String,
        path: PathBuf,
        is_write: bool,
    },
}

/// Stored permission entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPermission {
    pub decision: PermissionDecision,
    pub granted_at: DateTime<Utc>,
}

/// Per-tool permissions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolPermissions {
    /// Path -> Permission mapping
    #[serde(default)]
    pub paths: HashMap<String, StoredPermission>,
}

/// Project permissions file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectPermissions {
    pub version: u32,
    pub project_id: String,
    #[serde(default)]
    pub permissions: HashMap<String, ToolPermissions>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProjectPermissions {
    fn new(project_id: &str) -> Self {
        let now = Utc::now();
        Self {
            version: 1,
            project_id: project_id.to_string(),
            permissions: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

/// Manager for tool permissions
pub struct PermissionManager {
    /// Path to permissions file (.pangu/permissions.json)
    permissions_path: PathBuf,
    /// Loaded permissions
    permissions: ProjectPermissions,
    /// Project root for path checking
    project_root: PathBuf,
}

impl PermissionManager {
    /// Create a new permission manager for the given context
    pub fn new(context: &ToolContext) -> Self {
        let pangu_dir = context.project_root.join(".pangu");
        let permissions_path = pangu_dir.join("permissions.json");

        // Create .pangu directory if it doesn't exist
        if !pangu_dir.exists() {
            if let Err(e) = fs::create_dir_all(&pangu_dir) {
                warn!("Failed to create .pangu directory: {}", e);
            }
        }

        // Load existing permissions or create new
        let permissions = Self::load_permissions(&permissions_path, &context.project_id);

        Self {
            permissions_path,
            permissions,
            project_root: context.project_root.clone(),
        }
    }

    /// Load permissions from file or create new
    fn load_permissions(path: &Path, project_id: &str) -> ProjectPermissions {
        if path.exists() {
            match fs::read_to_string(path) {
                Ok(content) => {
                    match serde_json::from_str::<ProjectPermissions>(&content) {
                        Ok(perms) => {
                            debug!("Loaded permissions from {:?}", path);
                            return perms;
                        }
                        Err(e) => {
                            warn!("Failed to parse permissions file: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to read permissions file: {}", e);
                }
            }
        }

        debug!("Creating new permissions for project {}", project_id);
        ProjectPermissions::new(project_id)
    }

    /// Save permissions to file
    pub fn save(&mut self) -> Result<(), std::io::Error> {
        self.permissions.updated_at = Utc::now();

        let content = serde_json::to_string_pretty(&self.permissions)?;
        fs::write(&self.permissions_path, content)?;

        info!("Saved permissions to {:?}", self.permissions_path);
        Ok(())
    }

    /// Check if a path is within the project
    pub fn is_within_project(&self, path: &Path) -> bool {
        // Canonicalize both paths
        let canonical_root = match self.project_root.canonicalize() {
            Ok(p) => p,
            Err(_) => return false,
        };

        let canonical_path = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                // Path doesn't exist, try parent
                if let Some(parent) = path.parent() {
                    match parent.canonicalize() {
                        Ok(p) => p.join(path.file_name().unwrap_or_default()),
                        Err(_) => return false,
                    }
                } else {
                    return false;
                }
            }
        };

        canonical_path.starts_with(&canonical_root)
    }

    /// Check permission for a tool operation
    pub fn check(
        &self,
        tool_name: &str,
        path: &Path,
        is_write: bool,
    ) -> PermissionCheckResult {
        // For read operations within project, always allow
        if !is_write && self.is_within_project(path) {
            return PermissionCheckResult::Allowed;
        }

        // Check stored permissions
        let path_key = self.path_to_key(path);

        if let Some(tool_perms) = self.permissions.permissions.get(tool_name) {
            if let Some(stored) = tool_perms.paths.get(&path_key) {
                match stored.decision {
                    PermissionDecision::AlwaysAllow => {
                        debug!(
                            "Permission granted from stored: {} for {}",
                            tool_name, path_key
                        );
                        return PermissionCheckResult::Allowed;
                    }
                    PermissionDecision::Deny => {
                        debug!(
                            "Permission denied from stored: {} for {}",
                            tool_name, path_key
                        );
                        return PermissionCheckResult::Denied;
                    }
                    PermissionDecision::AllowOnce => {
                        // AllowOnce is not persisted, shouldn't be here
                        // but treat as needing permission
                    }
                }
            }
        }

        // Need user permission
        PermissionCheckResult::NeedsPermission {
            tool_name: tool_name.to_string(),
            path: path.to_path_buf(),
            is_write,
        }
    }

    /// Store a permission decision (only for AlwaysAllow)
    pub fn store_permission(
        &mut self,
        tool_name: &str,
        path: &Path,
        decision: PermissionDecision,
    ) {
        // Only persist AlwaysAllow decisions
        if decision != PermissionDecision::AlwaysAllow {
            return;
        }

        let path_key = self.path_to_key(path);

        let tool_perms = self
            .permissions
            .permissions
            .entry(tool_name.to_string())
            .or_default();

        tool_perms.paths.insert(
            path_key.clone(),
            StoredPermission {
                decision,
                granted_at: Utc::now(),
            },
        );

        info!(
            "Stored permission for {}: {} = {:?}",
            tool_name, path_key, decision
        );

        // Save to file
        if let Err(e) = self.save() {
            warn!("Failed to save permissions: {}", e);
        }
    }

    /// Convert path to storage key (canonicalized string)
    fn path_to_key(&self, path: &Path) -> String {
        path.canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_string()
    }

    /// Clear all permissions (for testing or reset)
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.permissions.permissions.clear();
        if let Err(e) = self.save() {
            warn!("Failed to save cleared permissions: {}", e);
        }
    }

    /// Get the .pangu directory path
    pub fn pangu_dir(&self) -> PathBuf {
        self.project_root.join(".pangu")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_context(dir: &Path) -> ToolContext {
        ToolContext::new(dir.to_path_buf(), "test123".to_string())
    }

    #[test]
    fn test_within_project() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());
        let manager = PermissionManager::new(&context);

        // Create a file within project
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "test").unwrap();

        assert!(manager.is_within_project(&file_path));
        assert!(!manager.is_within_project(Path::new("/etc/passwd")));
    }

    #[test]
    fn test_read_within_project_allowed() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());
        let manager = PermissionManager::new(&context);

        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "test").unwrap();

        let result = manager.check("read_file", &file_path, false);
        assert!(matches!(result, PermissionCheckResult::Allowed));
    }

    #[test]
    fn test_read_outside_project_needs_permission() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());
        let manager = PermissionManager::new(&context);

        let result = manager.check("read_file", Path::new("/etc/hosts"), false);
        assert!(matches!(result, PermissionCheckResult::NeedsPermission { .. }));
    }

    #[test]
    fn test_store_and_check_permission() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());
        let mut manager = PermissionManager::new(&context);

        // Create the external path for testing (file must exist for canonicalization)
        let external = temp.path().parent().unwrap().join("external.txt");
        fs::write(&external, "test").unwrap();

        // Initially needs permission
        let result = manager.check("read_file", &external, false);
        assert!(matches!(result, PermissionCheckResult::NeedsPermission { .. }));

        // Store always allow
        manager.store_permission("read_file", &external, PermissionDecision::AlwaysAllow);

        // Now should be allowed
        let result = manager.check("read_file", &external, false);
        assert!(matches!(result, PermissionCheckResult::Allowed));

        // Cleanup
        fs::remove_file(&external).ok();
    }
}
