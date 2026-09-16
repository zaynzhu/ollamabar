// 明文配置（用户已知情接受，spec 决策3）：写入尽力设 owner-only 权限
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KeyConfig { pub alias: String, pub api_key: String }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    pub keys: Vec<KeyConfig>,
    pub poll_interval_secs: u64,
}

impl Default for AppConfig {
    fn default() -> Self { AppConfig { keys: vec![], poll_interval_secs: 60 } }
}

pub fn load(path: &Path) -> std::io::Result<AppConfig> {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AppConfig::default()),
        Err(e) => Err(e),
    }
}

pub fn save(path: &Path, cfg: &AppConfig) -> std::io::Result<()> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(path, serde_json::to_string_pretty(cfg)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    // Windows：无简洁的可移植 owner-only 方案，接受明文本就是决策3的立场
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 配置读写往返() {
        let dir = std::env::temp_dir().join(format!("ollamabar-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let cfg = AppConfig { keys: vec![KeyConfig { alias: "工作".into(), api_key: "sk-x".into() }], poll_interval_secs: 60 };
        save(&path, &cfg).unwrap();
        assert_eq!(load(&path).unwrap(), cfg);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn 缺文件返回默认空配置() {
        let path = std::env::temp_dir().join("ollamabar-nonexist-xyz.json");
        assert!(load(&path).unwrap().keys.is_empty());
    }
}