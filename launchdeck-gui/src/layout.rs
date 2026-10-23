//! Launchpad 布局数据模型（复刻原版 LaunchpadGroup / LaunchpadDataManager）
//!
//! 结构：多页 → 每页若干 Item（App 或 Folder）

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 应用引用（持久化只存 bundle_id + 路径，启动时按路径重新解析）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppRef {
    pub bundle_id: String,
    pub name: String,
    pub path: String,
}

/// 文件夹
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub apps: Vec<AppRef>,
}

/// 页面中的一个格子
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Item {
    App { app: AppRef },
    Folder { folder: Folder },
}

impl Item {
    pub fn app(app: AppRef) -> Self { Item::App { app } }
    pub fn folder(f: Folder) -> Self { Item::Folder { folder: f } }

    pub fn bundle_id(&self) -> &str {
        match self {
            Item::App { app } => &app.bundle_id,
            Item::Folder { folder } => &folder.id,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Item::App { app } => &app.name,
            Item::Folder { folder } => &folder.name,
        }
    }

    /// 该格子包含的所有应用
    pub fn apps(&self) -> Vec<AppRef> {
        match self {
            Item::App { app } => vec![app.clone()],
            Item::Folder { folder } => folder.apps.clone(),
        }
    }

}

/// 一页
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Page {
    pub items: Vec<Item>,
}

/// 排序模式（对应原版 Sort By）
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SortMode {
    Custom,      // 自定义（可拖拽）
    Name,        // 名称
    DateAdded,   // 添加日期
    LastUsed,    // 上次使用
}

impl Default for SortMode {
    fn default() -> Self { SortMode::Custom }
}

impl SortMode {
    pub fn label(&self) -> &'static str {
        match self {
            SortMode::Custom => "自定义",
            SortMode::Name => "名称",
            SortMode::DateAdded => "添加日期",
            SortMode::LastUsed => "上次使用",
        }
    }
}

/// 浏览样式（分页 / 滚动）
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowsingStyle {
    Paged,
    Scroll,
}

impl Default for BrowsingStyle {
    fn default() -> Self { BrowsingStyle::Paged }
}

/// 完整布局
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchLayout {
    pub pages: Vec<Page>,
    /// 隐藏的应用 bundle_id
    pub hidden: HashSet<String>,
    /// 最后使用时间（bundle_id → unix ts）
    pub last_used: std::collections::HashMap<String, u64>,
    /// 添加时间（bundle_id → unix ts），用于「添加日期」排序
    pub added_at: std::collections::HashMap<String, u64>,
    pub sort_mode: SortMode,
    pub ascending: bool,
    pub browsing_style: BrowsingStyle,
    pub columns: usize,
    pub rows: usize,
    pub hide_names: bool,
    /// 是否来自原版 LaunchOS 布局导入（导入后不自动追加新应用）
    #[serde(default)]
    pub imported: bool,
    /// 版本号，用于兼容判断
    pub version: u32,
}

impl Default for LaunchLayout {
    fn default() -> Self {
        Self {
            pages: vec![Page::default()],
            hidden: HashSet::new(),
            last_used: Default::default(),
            added_at: Default::default(),
            sort_mode: SortMode::Custom,
            ascending: true,
            browsing_style: BrowsingStyle::Paged,
            columns: 7,
            rows: 5,
            hide_names: false,
            imported: false,
            version: 1,
        }
    }
}

impl LaunchLayout {
    pub fn page_count(&self) -> usize {
        self.pages.len().max(1)
    }

    pub fn all_bundle_ids(&self) -> HashSet<String> {
        let mut set = HashSet::new();
        for p in &self.pages {
            for it in &p.items {
                for a in it.apps() {
                    set.insert(a.bundle_id.clone());
                }
            }
        }
        set
    }

    /// 移除某应用（从所有页面/文件夹）
    pub fn remove_app(&mut self, bundle_id: &str) {
        for page in &mut self.pages {
            page.items.retain_mut(|it| match it {
                Item::App { app } => app.bundle_id != bundle_id,
                Item::Folder { folder } => {
                    folder.apps.retain(|a| a.bundle_id != bundle_id);
                    // 文件夹少于 2 个应用则解散
                    folder.apps.len() >= 2
                }
            });
        }
        self.hidden.remove(bundle_id);
    }

    /// 容量
    pub fn capacity(&self) -> usize { self.columns * self.rows }
}
