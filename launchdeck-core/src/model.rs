use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

/// 应用信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub id: String,                    // bundle identifier
    pub name: String,                  // 显示名称
    pub path: PathBuf,                 // .app 路径
    pub version: String,               // 版本号
    pub bundle_id: String,             // bundle identifier
    pub icon_path: Option<PathBuf>,    // 图标路径
    pub category: Option<String>,      // 分类
    pub size: u64,                     // 大小（字节）
    pub modified: SystemTime,          // 修改时间
    pub is_hidden: bool,               // 是否隐藏
    pub is_system: bool,               // 是否系统应用
    /// 该条目是替身文件/符号链接而非真实 .app 目录
    #[serde(default)]
    pub is_alias: bool,
    /// 替身文件本身的路径（is_alias 为 true 时指向替身，path 已解析为目标）
    #[serde(default)]
    pub alias_path: Option<String>,
}

/// 组/文件夹信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
    pub app_ids: Vec<String>,          // 包含的应用 IDs
    pub position: Option<(f32, f32)>,  // 位置
    pub page: usize,                   // 页码
}

/// 应用设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub display_mode: DisplayMode,
    pub hotkey: Option<HotkeyConfig>,
    pub show_dock_icon: bool,
    pub show_menu_bar_icon: bool,
    pub launch_at_login: bool,
    pub auto_update: bool,
    pub theme: Theme,
    pub grid_size: GridSize,
    pub animation_enabled: bool,
    /// 触发角
    pub hot_corner: HotCorner,
    /// 启动应用后自动隐藏启动台
    pub hide_after_launch: bool,
    /// 背景类型
    pub background: BackgroundType,
    /// 图标样式
    pub icon_style: IconStyle,
    /// 在搜索结果中隐藏隐藏应用
    pub hide_hidden_in_search: bool,
    /// 启动台显示在
    pub show_on: ScreenChoice,
    /// 重新打开时回到主页面
    pub return_home: ReturnHome,
    /// 按住 Option 进入卸载模式
    pub option_uninstall: bool,
    /// 自动下载更新
    pub auto_download: bool,
    /// 排序时同时排序文件夹内的应用
    #[serde(default)]
    pub sort_folders_too: bool,
    /// 排序时 Apple 应用相对用户应用的位置
    #[serde(default)]
    pub apple_placement: ApplePlacement,
    /// 用户自定义扫描目录（在默认目录之外追加）
    #[serde(default)]
    pub custom_sources: Vec<String>,
    /// 触控板捏合手势打开启动台
    #[serde(default)]
    pub pinch_gesture: PinchGesture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundType {
    Wallpaper,
    Frosted,
}

impl Default for BackgroundType {
    fn default() -> Self { BackgroundType::Wallpaper }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconStyle {
    Rounded,
    Circle,
    Square,
    Custom,
}

impl Default for IconStyle {
    fn default() -> Self { IconStyle::Rounded }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScreenChoice {
    Main,
    Cursor,
    Dock,
    Active,
}

impl Default for ScreenChoice {
    fn default() -> Self { ScreenChoice::Main }
}

impl ScreenChoice {
    pub fn label(&self) -> &'static str {
        match self {
            ScreenChoice::Main => "主屏幕",
            ScreenChoice::Cursor => "鼠标所在屏幕",
            ScreenChoice::Dock => "Dock 所在屏幕",
            ScreenChoice::Active => "当前激活屏幕",
        }
    }
}

/// Apple 应用在排序中的位置
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ApplePlacement {
    /// 排在用户应用之前
    Before,
    /// 排在用户应用之后
    After,
    /// 与其他应用混排
    #[default]
    Mixed,
}

impl ApplePlacement {
    pub fn label(&self) -> &'static str {
        match self {
            ApplePlacement::Before => "应用之前",
            ApplePlacement::After => "应用之后",
            ApplePlacement::Mixed => "与其他应用混排",
        }
    }
}

/// 触控板捏合手势
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PinchGesture {
    #[default]
    Off,
    Three,
    FourOrFive,
    Five,
}

impl PinchGesture {
    pub fn label(&self) -> &'static str {
        match self {
            PinchGesture::Off => "关",
            PinchGesture::Three => "三指捏合",
            PinchGesture::FourOrFive => "四指或五指捏合",
            PinchGesture::Five => "五指捏合",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReturnHome {
    Never,
    Always,
    FiveMinutes,
    OneMinute,
    ThirtySeconds,
    FifteenSeconds,
    FiveSeconds,
}

impl Default for ReturnHome {
    fn default() -> Self { ReturnHome::FiveMinutes }
}

impl ReturnHome {
    pub fn label(&self) -> &'static str {
        match self {
            ReturnHome::Never => "从不",
            ReturnHome::Always => "始终",
            ReturnHome::FiveMinutes => "5 分钟后",
            ReturnHome::OneMinute => "1 分钟后",
            ReturnHome::ThirtySeconds => "30 秒后",
            ReturnHome::FifteenSeconds => "15 秒后",
            ReturnHome::FiveSeconds => "5 秒后",
        }
    }

    /// 关闭超过该时长才回首页；None = 不按时长（始终/从不）
    pub fn threshold(&self) -> Option<std::time::Duration> {
        match self {
            ReturnHome::Never | ReturnHome::Always => None,
            ReturnHome::FiveMinutes => Some(std::time::Duration::from_secs(300)),
            ReturnHome::OneMinute => Some(std::time::Duration::from_secs(60)),
            ReturnHome::ThirtySeconds => Some(std::time::Duration::from_secs(30)),
            ReturnHome::FifteenSeconds => Some(std::time::Duration::from_secs(15)),
            ReturnHome::FiveSeconds => Some(std::time::Duration::from_secs(5)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DisplayMode {
    Window,
    Fullscreen,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub key_code: u16,
    pub modifiers: u32,
}

/// 触发角位置
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HotCorner {
    None,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Default for HotCorner {
    fn default() -> Self {
        HotCorner::None
    }
}

impl HotCorner {
    pub fn label(&self) -> &'static str {
        match self {
            HotCorner::None => "关",
            HotCorner::TopLeft => "左上角",
            HotCorner::TopRight => "右上角",
            HotCorner::BottomLeft => "左下角",
            HotCorner::BottomRight => "右下角",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GridSize {
    Small,
    Medium,
    Large,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            display_mode: DisplayMode::Fullscreen,
            hotkey: Some(HotkeyConfig { key_code: 49, modifiers: 0x2000 }), // Option+Space
            show_dock_icon: true,
            show_menu_bar_icon: true,
            launch_at_login: false,
            auto_update: true,
            theme: Theme::System,
            grid_size: GridSize::Medium,
            animation_enabled: true,
            hot_corner: HotCorner::None,
            hide_after_launch: true,
            background: BackgroundType::Wallpaper,
            icon_style: IconStyle::Rounded,
            hide_hidden_in_search: true,
            show_on: ScreenChoice::Main,
            return_home: ReturnHome::FiveMinutes,
            option_uninstall: true,
            auto_download: false,
            sort_folders_too: false,
            apple_placement: ApplePlacement::Mixed,
            custom_sources: Vec::new(),
            pinch_gesture: PinchGesture::Off,
        }
    }
}
