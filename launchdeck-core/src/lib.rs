//! LaunchDeck Core - 核心业务逻辑库
//!
//! 提供应用扫描、配置管理等核心功能。

pub mod error;
pub mod model;
pub mod config;
pub mod scanner;
pub mod util;

pub use error::{LaunchDeckError, Result};
pub use model::*;
pub use config::*;
pub use scanner::AppScanner;
pub use util::*;

/// 初始化核心库
pub fn init() -> Result<()> {
    config::init()
}

/// 版本信息
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_HASH: &str = "unknown";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        assert!(init().is_ok());
    }

    #[test]
    fn test_settings() {
        init().unwrap();
        let settings = get_settings();
        assert_eq!(settings.theme, crate::model::Theme::System);
    }
}
