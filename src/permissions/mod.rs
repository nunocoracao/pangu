//! Permission system for tool execution
//!
//! Manages user permissions for tool calls, stored per-project in `.pangu/permissions.json`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Permission decision for a tool call
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// Always allow this tool call
    Always,
    /// Never allow this tool call
    Never,
    /// Ask the user each time
    Ask,
}

impl Default for Permission {
    fn default() -> Self {
        Permission::Ask
    }
}

/// A permission rule for a specific tool and pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    /// The tool name (e.g., "fetch")
    pub tool: String,
    /// The pattern to match (e.g., "github.com", "*.example.com", "*")
    pub pattern: String,
    /// The permission decision
    pub permission: Permission,
}

/// Stored permissions for a project
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionStore {
    /// Version for future compatibility
    pub version: u32,
    /// Rules organized by tool name, then pattern
    #[serde(default)]
    pub rules: HashMap<String, HashMap<String, Permission>>,
}

impl PermissionStore {
    pub fn new() -> Self {
        Self {
            version: 1,
            rules: HashMap::new(),
        }
    }
}

/// Permission manager that handles checking and storing permissions
pub struct PermissionManager {
    /// Path to the permissions file
    permissions_path: PathBuf,
    /// Cached permissions
    store: PermissionStore,
    /// One-time permissions for this session (tool -> pattern -> permission)
    session_permissions: HashMap<String, HashMap<String, Permission>>,
}

impl PermissionManager {
    /// Create a new permission manager for the current directory
    pub fn new() -> Self {
        let permissions_path = Self::find_permissions_path();
        let store = Self::load_store(&permissions_path);

        Self {
            permissions_path,
            store,
            session_permissions: HashMap::new(),
        }
    }

    /// Find the permissions file path (creates .pangu directory if needed)
    fn find_permissions_path() -> PathBuf {
        let pangu_dir = PathBuf::from(".pangu");
        if !pangu_dir.exists() {
            let _ = fs::create_dir_all(&pangu_dir);
        }
        pangu_dir.join("permissions.json")
    }

    /// Load permissions from file
    fn load_store(path: &Path) -> PermissionStore {
        if path.exists() {
            match fs::read_to_string(path) {
                Ok(contents) => {
                    serde_json::from_str(&contents).unwrap_or_default()
                }
                Err(_) => PermissionStore::new(),
            }
        } else {
            PermissionStore::new()
        }
    }

    /// Save permissions to file
    fn save_store(&self) -> Result<(), std::io::Error> {
        let contents = serde_json::to_string_pretty(&self.store)?;
        fs::write(&self.permissions_path, contents)
    }

    /// Check permission for a tool call
    /// Returns the permission and whether it's a stored (persistent) permission
    pub fn check_permission(&self, tool: &str, params: &str) -> (Permission, bool) {
        // Extract the key from params (e.g., domain from URL for fetch)
        let key = self.extract_key(tool, params);

        // First check session permissions (one-time allows/denies)
        if let Some(tool_perms) = self.session_permissions.get(tool) {
            // Check exact match
            if let Some(perm) = tool_perms.get(&key) {
                return (*perm, false);
            }
        }

        // Then check stored permissions
        if let Some(tool_perms) = self.store.rules.get(tool) {
            // Check exact match first
            if let Some(perm) = tool_perms.get(&key) {
                return (*perm, true);
            }

            // Check wildcard patterns
            for (pattern, perm) in tool_perms {
                if self.matches_pattern(pattern, &key) {
                    return (*perm, true);
                }
            }

            // Check global wildcard
            if let Some(perm) = tool_perms.get("*") {
                return (*perm, true);
            }
        }

        // Default: ask
        (Permission::Ask, false)
    }

    /// Extract the key to match against from tool params
    fn extract_key(&self, tool: &str, params: &str) -> String {
        match tool {
            "fetch" => {
                // Extract domain from URL
                if let Ok(url) = url::Url::parse(params) {
                    url.host_str().unwrap_or(params).to_string()
                } else {
                    params.to_string()
                }
            }
            "read_file" | "write_file" => {
                // Use the file path
                params.to_string()
            }
            "run_command" => {
                // Use the command (first word)
                params.split_whitespace().next().unwrap_or(params).to_string()
            }
            _ => params.to_string(),
        }
    }

    /// Check if a key matches a pattern (supports * wildcard and *.domain.com patterns)
    fn matches_pattern(&self, pattern: &str, key: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        if pattern.starts_with("*.") {
            // Wildcard subdomain match (e.g., *.github.com matches api.github.com)
            let domain_suffix = &pattern[1..]; // .github.com
            return key.ends_with(domain_suffix) || key == &pattern[2..];
        }

        pattern == key
    }

    /// Set a persistent permission (saved to disk)
    pub fn set_permission(&mut self, tool: &str, key: &str, permission: Permission) {
        let tool_perms = self.store.rules.entry(tool.to_string()).or_default();
        tool_perms.insert(key.to_string(), permission);

        // Save to disk
        if let Err(e) = self.save_store() {
            tracing::error!("Failed to save permissions: {}", e);
        }
    }

    /// Set a session-only permission (not saved to disk)
    pub fn set_session_permission(&mut self, tool: &str, key: &str, permission: Permission) {
        let tool_perms = self.session_permissions.entry(tool.to_string()).or_default();
        tool_perms.insert(key.to_string(), permission);
    }

    /// Get the key that would be used for a tool call (for display purposes)
    pub fn get_display_key(&self, tool: &str, params: &str) -> String {
        self.extract_key(tool, params)
    }
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// User's response to a permission prompt
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionResponse {
    /// Allow this one time
    AllowOnce,
    /// Always allow this pattern
    AllowAlways,
    /// Deny this one time
    DenyOnce,
    /// Never allow this pattern
    DenyAlways,
}

impl PermissionResponse {
    /// Convert response to permission and whether it should be persisted
    pub fn to_permission_and_persist(self) -> (Permission, bool) {
        match self {
            PermissionResponse::AllowOnce => (Permission::Always, false),
            PermissionResponse::AllowAlways => (Permission::Always, true),
            PermissionResponse::DenyOnce => (Permission::Never, false),
            PermissionResponse::DenyAlways => (Permission::Never, true),
        }
    }
}
