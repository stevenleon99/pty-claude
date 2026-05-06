//! Environment resolver - builds effective environments for sessions
//!
//! Supports three modes:
//! - Shell: minimal env, injected by login shell
//! - Clean: only base vars + .env + overrides
//! - BootstrapFromShell: capture login shell env, then layer overrides

use std::collections::HashMap;
use std::path::Path;

use crate::session::env::{
    EffectiveEnvironment, EnvConfig, EnvEntry, EnvMode, EnvSource,
};
use crate::session::env_file_parser::parse_env_file;
use crate::store::host_config::HostIdentity;

/// Result of environment resolution.
pub type EffectiveEnvResult = Result<EffectiveEnvironment, String>;

/// Resolve a complete environment from config + context.
pub fn resolve_environment(
    config: &EnvConfig,
    workspace_root: &str,
    host_config: &HostIdentity,
    provider_overrides: &HashMap<String, String>,
) -> EffectiveEnvResult {
    match config.mode {
        EnvMode::Shell => resolve_shell_env(config, workspace_root, provider_overrides),
        EnvMode::Clean => resolve_clean_env(config, workspace_root, provider_overrides),
        EnvMode::BootstrapFromShell => {
            resolve_bootstrap_env(config, workspace_root, host_config, provider_overrides)
        }
    }
}

/// Resolve the shell to use for bootstrap.
pub fn resolve_bootstrap_shell(host_config: &HostIdentity) -> String {
    if let Some(ref path) = host_config.bootstrap_shell_path {
        if !path.is_empty() {
            return path.clone();
        }
    }
    std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/bin/zsh".to_string()
        } else {
            "/bin/bash".to_string()
        }
    })
}

fn resolve_shell_env(
    config: &EnvConfig,
    workspace_root: &str,
    provider_overrides: &HashMap<String, String>,
) -> EffectiveEnvResult {
    let mut result = EffectiveEnvironment {
        mode: EnvMode::Shell,
        bootstrap_shell_path: Some(resolve_bootstrap_shell(&HostIdentity::default())),
        ..Default::default()
    };

    // Load .env files
    let env_file_paths = resolve_env_file_paths(config.env_file_path.as_deref(), workspace_root);
    for path in &env_file_paths {
        let current_map = entries_to_map(&result.entries);
        if let Ok(content) = std::fs::read_to_string(path) {
            let pairs = parse_env_file(&content, &current_map);
            for (k, v) in pairs {
                upsert_entry(&mut result.entries, k, v, EnvSource::EnvFile);
            }
        }
    }
    if !env_file_paths.is_empty() {
        result.env_file_path = Some(env_file_paths.last().unwrap().to_string());
    }

    apply_overrides(&mut result.entries, provider_overrides, EnvSource::ProviderConfig);
    apply_overrides(&mut result.entries, &config.overrides, EnvSource::SessionOverride);

    Ok(result)
}

fn resolve_clean_env(
    config: &EnvConfig,
    workspace_root: &str,
    provider_overrides: &HashMap<String, String>,
) -> EffectiveEnvResult {
    let mut result = EffectiveEnvironment {
        mode: EnvMode::Clean,
        ..Default::default()
    };

    // Layer 1: minimal base vars
    let minimal_path = "/usr/bin:/bin:/usr/sbin:/sbin";
    result.entries.push(EnvEntry {
        key: "PATH".to_string(),
        value: minimal_path.to_string(),
        source: EnvSource::DaemonInherited,
    });
    if let Ok(home) = std::env::var("HOME") {
        result.entries.push(EnvEntry {
            key: "HOME".to_string(),
            value: home,
            source: EnvSource::DaemonInherited,
        });
    }
    if let Ok(user) = std::env::var("USER") {
        result.entries.push(EnvEntry {
            key: "USER".to_string(),
            value: user,
            source: EnvSource::DaemonInherited,
        });
    }
    if let Ok(tmpdir) = std::env::var("TMPDIR") {
        result.entries.push(EnvEntry {
            key: "TMPDIR".to_string(),
            value: tmpdir,
            source: EnvSource::DaemonInherited,
        });
    }

    // Layer 2: .env file
    let env_file_paths = resolve_env_file_paths(config.env_file_path.as_deref(), workspace_root);
    for path in &env_file_paths {
        let current_map = entries_to_map(&result.entries);
        if let Ok(content) = std::fs::read_to_string(path) {
            let pairs = parse_env_file(&content, &current_map);
            for (k, v) in pairs {
                upsert_entry(&mut result.entries, k, v, EnvSource::EnvFile);
            }
        }
        result.env_file_path = Some(path.to_string());
    }

    // Layer 3: ProviderConfig overrides
    apply_overrides(&mut result.entries, provider_overrides, EnvSource::ProviderConfig);

    // Layer 4: session-level overrides
    apply_overrides(&mut result.entries, &config.overrides, EnvSource::SessionOverride);

    Ok(result)
}

fn resolve_bootstrap_env(
    config: &EnvConfig,
    workspace_root: &str,
    host_config: &HostIdentity,
    provider_overrides: &HashMap<String, String>,
) -> EffectiveEnvResult {
    let shell_path = resolve_bootstrap_shell(host_config);

    let mut result = EffectiveEnvironment {
        mode: EnvMode::BootstrapFromShell,
        bootstrap_shell_path: Some(shell_path.clone()),
        ..Default::default()
    };

    // Layer 1: bootstrap-captured env (placeholder - actual bootstrap requires Unix)
    // In a real implementation, this would run `shell -l -c 'env -0'` and parse the output.
    // For now, we inherit the current environment as the bootstrap base.
    for (key, value) in std::env::vars() {
        result.entries.push(EnvEntry {
            key,
            value,
            source: EnvSource::BootstrapShell,
        });
    }

    // Layer 2: service-manager imported env
    if host_config.import_service_manager_environment
        && !host_config.service_manager_environment_allowlist.is_empty()
    {
        for key in &host_config.service_manager_environment_allowlist {
            if let Ok(value) = std::env::var(key) {
                upsert_entry(&mut result.entries, key.clone(), value, EnvSource::ServiceManager);
            }
        }
    }

    // Layer 3: .env file overrides
    let env_file_paths = resolve_env_file_paths(config.env_file_path.as_deref(), workspace_root);
    for path in &env_file_paths {
        let current_map = entries_to_map(&result.entries);
        if let Ok(content) = std::fs::read_to_string(path) {
            let pairs = parse_env_file(&content, &current_map);
            for (k, v) in pairs {
                upsert_entry(&mut result.entries, k, v, EnvSource::EnvFile);
            }
        }
    }
    if !env_file_paths.is_empty() {
        result.env_file_path = Some(env_file_paths.last().unwrap().to_string());
    }

    // Layer 4: ProviderConfig overrides
    apply_overrides(&mut result.entries, provider_overrides, EnvSource::ProviderConfig);

    // Layer 5: session-level overrides
    apply_overrides(&mut result.entries, &config.overrides, EnvSource::SessionOverride);

    Ok(result)
}

// --- Helpers ---

fn resolve_env_file_paths(explicit_path: Option<&str>, workspace_root: &str) -> Vec<String> {
    if let Some(path) = explicit_path {
        let p = Path::new(path);
        let resolved = if p.is_relative() {
            Path::new(workspace_root).join(p)
        } else {
            p.to_path_buf()
        };
        return vec![resolved.to_string_lossy().to_string()];
    }

    let mut paths = Vec::new();
    let ws = Path::new(workspace_root);
    let env_in = ws.join(".env.in");
    let env_file = ws.join(".env");
    if env_in.exists() {
        paths.push(env_in.to_string_lossy().to_string());
    }
    if env_file.exists() {
        paths.push(env_file.to_string_lossy().to_string());
    }
    paths
}

fn upsert_entry(entries: &mut Vec<EnvEntry>, key: String, value: String, source: EnvSource) {
    if let Some(entry) = entries.iter_mut().find(|e| e.key == key) {
        entry.value = value;
        entry.source = source;
    } else {
        entries.push(EnvEntry { key, value, source });
    }
}

fn apply_overrides(
    entries: &mut Vec<EnvEntry>,
    overrides: &HashMap<String, String>,
    source: EnvSource,
) {
    for (key, value) in overrides {
        upsert_entry(entries, key.clone(), value.clone(), source);
    }
}

fn entries_to_map(entries: &[EnvEntry]) -> HashMap<String, String> {
    entries
        .iter()
        .map(|e| (e.key.clone(), e.value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_shell_env_minimal() {
        let config = EnvConfig {
            mode: EnvMode::Shell,
            overrides: HashMap::new(),
            env_file_path: None,
        };
        let result = resolve_environment(
            &config,
            "/tmp",
            &HostIdentity::default(),
            &HashMap::new(),
        );
        assert!(result.is_ok());
        let env = result.unwrap();
        assert_eq!(env.mode, EnvMode::Shell);
    }

    #[test]
    fn test_resolve_clean_env_base_vars() {
        let config = EnvConfig {
            mode: EnvMode::Clean,
            overrides: HashMap::new(),
            env_file_path: None,
        };
        let result = resolve_environment(
            &config,
            "/tmp",
            &HostIdentity::default(),
            &HashMap::new(),
        );
        assert!(result.is_ok());
        let env = result.unwrap();
        assert_eq!(env.mode, EnvMode::Clean);
        // Should have at least PATH
        assert!(env.entries.iter().any(|e| e.key == "PATH"));
    }

    #[test]
    fn test_resolve_with_overrides() {
        let mut overrides = HashMap::new();
        overrides.insert("MY_VAR".to_string(), "my_value".to_string());

        let mut env_overrides = HashMap::new();
        env_overrides.insert("SESSION_VAR".to_string(), "session_value".to_string());

        let config = EnvConfig {
            mode: EnvMode::Clean,
            overrides: env_overrides,
            env_file_path: None,
        };

        let result = resolve_environment(&config, "/tmp", &HostIdentity::default(), &overrides);
        assert!(result.is_ok());
        let env = result.unwrap();

        let my_var = env.entries.iter().find(|e| e.key == "MY_VAR").unwrap();
        assert_eq!(my_var.value, "my_value");
        assert_eq!(my_var.source, EnvSource::ProviderConfig);

        let session_var = env.entries.iter().find(|e| e.key == "SESSION_VAR").unwrap();
        assert_eq!(session_var.value, "session_value");
        assert_eq!(session_var.source, EnvSource::SessionOverride);
    }

    #[test]
    fn test_resolve_bootstrap_shell_default() {
        let identity = HostIdentity::default();
        let shell = resolve_bootstrap_shell(&identity);
        // Should resolve to something
        assert!(!shell.is_empty());
    }
}
