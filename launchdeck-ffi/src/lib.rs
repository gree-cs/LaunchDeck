//! LaunchDeck FFI - Swift 互操作层
//!
//! 提供 C 兼容的 API 供 Swift 调用

use launchdeck_core::*;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uchar};
use std::ptr;

/// 错误码
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LaunchDeckErrorCode {
    Success = 0,
    NullPointer = 1,
    InvalidUtf8 = 2,
    InternalError = 3,
    NotFound = 4,
    PermissionDenied = 5,
    NetworkError = 7,
}

/// C 兼容的应用信息结构
#[repr(C)]
pub struct CAppInfo {
    pub id: *mut c_char,
    pub name: *mut c_char,
    pub path: *mut c_char,
    pub version: *mut c_char,
    pub bundle_id: *mut c_char,
    pub size: u64,
    pub is_hidden: c_uchar,
    pub is_system: c_uchar,
}

/// C 兼容的应用设置结构
#[repr(C)]
pub struct CAppSettings {
    pub display_mode: c_int,      // 0=Window, 1=Fullscreen
    pub show_dock_icon: c_uchar,
    pub show_menu_bar_icon: c_uchar,
    pub launch_at_login: c_uchar,
    pub auto_update: c_uchar,
    pub theme: c_int,             // 0=System, 1=Light, 2=Dark
    pub grid_size: c_int,         // 0=Small, 1=Medium, 2=Large
    pub animation_enabled: c_uchar,
}


/// 释放 C 字符串
#[no_mangle]
pub unsafe extern "C" fn launchdeck_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { CString::from_raw(ptr); }
    }
}

/// 初始化核心库
#[no_mangle]
pub extern "C" fn launchdeck_init() -> c_int {
    match launchdeck_core::init() {
        Ok(_) => 0,
        Err(_) => 1,
    }
}

/// 扫描应用
#[no_mangle]
pub extern "C" fn launchdeck_scan_apps(out_apps: *mut *mut CAppInfo, out_count: *mut usize) -> c_int {
    if out_apps.is_null() || out_count.is_null() {
        return 1;
    }

    let scanner = launchdeck_core::AppScanner::default();
    match scanner.scan() {
        Ok(apps) => {
            let count = apps.len();
            let mut c_apps: Vec<CAppInfo> = Vec::with_capacity(count);

            for app in apps {
                let id = CString::new(app.id).unwrap();
                let name = CString::new(app.name).unwrap();
                let path = CString::new(app.path.to_string_lossy().to_string()).unwrap();
                let version = CString::new(app.version).unwrap();
                let bundle_id = CString::new(app.bundle_id).unwrap();

                c_apps.push(CAppInfo {
                    id: id.into_raw(),
                    name: name.into_raw(),
                    path: path.into_raw(),
                    version: version.into_raw(),
                    bundle_id: bundle_id.into_raw(),
                    size: app.size,
                    is_hidden: app.is_hidden as c_uchar,
                    is_system: app.is_system as c_uchar,
                });
            }

            let boxed = c_apps.into_boxed_slice();
            let ptr = boxed.as_ptr() as *mut CAppInfo;
            std::mem::forget(boxed);

            unsafe {
                *out_apps = ptr;
                *out_count = count;
            }
            0
        }
        Err(_) => 1,
    }
}

/// 释放应用列表
#[no_mangle]
pub unsafe extern "C" fn launchdeck_free_apps(apps: *mut CAppInfo, count: usize) {
    if apps.is_null() || count == 0 {
        return;
    }
    let slice = unsafe { std::slice::from_raw_parts_mut(apps, count) };
    for app in slice.iter_mut() {
        if !app.id.is_null() { unsafe { CString::from_raw(app.id); } }
        if !app.name.is_null() { unsafe { CString::from_raw(app.name); } }
        if !app.path.is_null() { unsafe { CString::from_raw(app.path); } }
        if !app.version.is_null() { unsafe { CString::from_raw(app.version); } }
        if !app.bundle_id.is_null() { unsafe { CString::from_raw(app.bundle_id); } }
    }
    unsafe {
        drop(Box::from_raw(apps));
    }
}

/// 获取设置
#[no_mangle]
pub extern "C" fn launchdeck_get_settings(out_settings: *mut CAppSettings) -> c_int {
    if out_settings.is_null() {
        return 1;
    }

    let settings = launchdeck_core::get_settings();

    unsafe {
        *out_settings = CAppSettings {
            display_mode: match settings.display_mode {
                launchdeck_core::model::DisplayMode::Window => 0,
                launchdeck_core::model::DisplayMode::Fullscreen => 1,
            },
            show_dock_icon: settings.show_dock_icon as c_uchar,
            show_menu_bar_icon: settings.show_menu_bar_icon as c_uchar,
            launch_at_login: settings.launch_at_login as c_uchar,
            auto_update: settings.auto_update as c_uchar,
            theme: match settings.theme {
                launchdeck_core::model::Theme::System => 0,
                launchdeck_core::model::Theme::Light => 1,
                launchdeck_core::model::Theme::Dark => 2,
            },
            grid_size: match settings.grid_size {
                launchdeck_core::model::GridSize::Small => 0,
                launchdeck_core::model::GridSize::Medium => 1,
                launchdeck_core::model::GridSize::Large => 2,
            },
            animation_enabled: settings.animation_enabled as c_uchar,
        };
    }
    0
}

/// 更新设置
#[no_mangle]
pub extern "C" fn launchdeck_update_settings(settings: *const CAppSettings) -> c_int {
    if settings.is_null() {
        return 1;
    }

    let c_settings = unsafe { &*settings };

    let mut settings_rs = launchdeck_core::get_settings();

    settings_rs.display_mode = match c_settings.display_mode {
        0 => launchdeck_core::model::DisplayMode::Window,
        1 => launchdeck_core::model::DisplayMode::Fullscreen,
        _ => launchdeck_core::model::DisplayMode::Window,
    };
    settings_rs.show_dock_icon = c_settings.show_dock_icon != 0;
    settings_rs.show_menu_bar_icon = c_settings.show_menu_bar_icon != 0;
    settings_rs.launch_at_login = c_settings.launch_at_login != 0;
    settings_rs.auto_update = c_settings.auto_update != 0;
    settings_rs.theme = match c_settings.theme {
        0 => launchdeck_core::model::Theme::System,
        1 => launchdeck_core::model::Theme::Light,
        2 => launchdeck_core::model::Theme::Dark,
        _ => launchdeck_core::model::Theme::System,
    };
    settings_rs.grid_size = match c_settings.grid_size {
        0 => launchdeck_core::model::GridSize::Small,
        1 => launchdeck_core::model::GridSize::Medium,
        2 => launchdeck_core::model::GridSize::Large,
        _ => launchdeck_core::model::GridSize::Medium,
    };
    settings_rs.animation_enabled = c_settings.animation_enabled != 0;

    match launchdeck_core::save_settings(&settings_rs) {
        Ok(_) => 0,
        Err(_) => 1,
    }
}
