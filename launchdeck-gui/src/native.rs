//! macOS 原生集成：全局快捷键、触发角、开机自启动
//!
//! 快捷键使用 Carbon `RegisterEventHotKey`（Cocoa 应用可直接使用，
//! 无需辅助功能权限）。触发角使用 CoreGraphics 轮询鼠标位置。

use launchdeck_core::{HotCorner, ScreenChoice};
use std::ffi::c_void;
use std::os::raw::{c_char, c_int};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

static HOTKEY_PRESSED: AtomicBool = AtomicBool::new(false);
static F4_PRESSED: AtomicBool = AtomicBool::new(false);
static F4_OK: AtomicBool = AtomicBool::new(false);
static SWIPE_DIR: AtomicI32 = AtomicI32::new(0);
static SWIPE_ACC: AtomicI32 = AtomicI32::new(0);
static SWIPE_OK: AtomicBool = AtomicBool::new(false);
static CORNER_ZONE: AtomicU8 = AtomicU8::new(0); // 0=关
static CORNER_EXIT: AtomicBool = AtomicBool::new(false);

static CTX: OnceLock<Mutex<Option<egui::Context>>> = OnceLock::new();

fn ctx_slot() -> &'static Mutex<Option<egui::Context>> {
    CTX.get_or_init(|| Mutex::new(None))
}

pub fn set_context(ctx: egui::Context) {
    if let Ok(mut slot) = ctx_slot().lock() {
        *slot = Some(ctx);
    }
}

pub fn request_repaint() {
    if let Ok(slot) = ctx_slot().lock() {
        if let Some(ctx) = slot.as_ref() {
            ctx.request_repaint();
        }
    }
}

pub fn take_hotkey() -> bool {
    HOTKEY_PRESSED.swap(false, Ordering::SeqCst)
}

pub fn take_corner() -> bool {
    if CORNER_EXIT.swap(false, Ordering::SeqCst) {
        true
    } else {
        false
    }
}

// ---------------------------------------------------------------- Carbon 快捷键

type OSStatus = i32;

#[repr(C)]
struct EventHotKeyID {
    signature: u32,
    id: u32,
}

#[repr(C)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn GetApplicationEventTarget() -> *mut c_void;
    fn InstallEventHandler(
        target: *mut c_void,
        handler: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> OSStatus>,
        num_types: usize,
        types: *const EventTypeSpec,
        user_data: *mut c_void,
        out_ref: *mut *mut c_void,
    ) -> OSStatus;
    fn RegisterEventHotKey(
        hot_key_code: u32,
        hot_key_modifiers: u32,
        hot_key_id: EventHotKeyID,
        target: *mut c_void,
        options: u32,
        out_ref: *mut *mut c_void,
    ) -> OSStatus;
    fn GetEventParameter(
        event: *mut c_void,
        name: u32,
        desired_type: u32,
        actual_type: *mut u32,
        buffer_size: u32,
        actual_size: *mut u32,
        data: *mut c_void,
    ) -> OSStatus;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: Option<unsafe extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void>,
        user_info: *mut c_void,
    ) -> *mut c_void;
    fn CGEventGetDoubleValueField(event: *mut c_void, field: u32) -> f64;
    fn CFMachPortCreateRunLoopSource(alloc: *mut c_void, port: *mut c_void, order: i64) -> *mut c_void;
    fn CFRunLoopGetCurrent() -> *mut c_void;
    fn CFRunLoopAddSource(rl: *mut c_void, source: *mut c_void, mode: *mut c_void);
    static kCFRunLoopCommonModes: *const c_void;
}

unsafe extern "C" fn hotkey_handler(
    _call: *mut c_void,
    event: *mut c_void,
    _user: *mut c_void,
) -> OSStatus {
    let mut hk = EventHotKeyID {
        signature: 0,
        id: 0,
    };
    let mut actual_type: u32 = 0;
    let mut actual_size: u32 = 0;
    let ok = GetEventParameter(
        event,
        0x2d2d_2d2d, // kEventParamDirectObject
        0x686b_6964, // typeEventHotKeyID
        &mut actual_type,
        std::mem::size_of::<EventHotKeyID>() as u32,
        &mut actual_size,
        &mut hk as *mut _ as *mut c_void,
    ) == 0;
    if std::env::var("LAUNCHDECK_NSDEBUG").is_ok() {
        eprintln!("[ns] hotkey handler ok={ok} id={}", hk.id);
    }
    if ok && hk.id == 2 {
        F4_PRESSED.store(true, Ordering::SeqCst);
    } else {
        HOTKEY_PRESSED.store(true, Ordering::SeqCst);
    }
    request_repaint();
    0
}

const K_EVENT_CLASS_KEYBOARD: u32 = 0x6b657962; // 'keyb'
const K_EVENT_HOTKEY_PRESSED: u32 = 5;
const OPTION_KEY: u32 = 2048;
const CONTROL_KEY: u32 = 4096;
const SHIFT_KEY: u32 = 512;
const CMD_KEY: u32 = 256;

/// 注册全局快捷键（默认 Option+Space）
pub fn register_hotkey(key_code: u16, modifiers: u32) -> bool {
    unsafe {
        let target = GetApplicationEventTarget();
        let spec = EventTypeSpec {
            event_class: K_EVENT_CLASS_KEYBOARD,
            event_kind: K_EVENT_HOTKEY_PRESSED,
        };
        let mut handler_ref: *mut c_void = std::ptr::null_mut();
        let status = InstallEventHandler(
            target,
            Some(hotkey_handler),
            1,
            &spec,
            std::ptr::null_mut(),
            &mut handler_ref,
        );
        if status != 0 {
            return false;
        }

        let mut carbon_mods = 0;
        if modifiers & 0x2000 != 0 {
            carbon_mods |= OPTION_KEY;
        }
        if modifiers & 0x1000 != 0 {
            carbon_mods |= CONTROL_KEY;
        }
        if modifiers & 0x0200 != 0 {
            carbon_mods |= SHIFT_KEY;
        }
        if modifiers & 0x0100 != 0 {
            carbon_mods |= CMD_KEY;
        }
        if carbon_mods == 0 {
            carbon_mods = OPTION_KEY;
        }

        let hot_id = EventHotKeyID {
            signature: 0x4C_4F_53_31, // 'LOS1'
            id: 1,
        };
        let mut hk_ref: *mut c_void = std::ptr::null_mut();
        RegisterEventHotKey(
            key_code as u32,
            carbon_mods,
            hot_id,
            target,
            0,
            &mut hk_ref,
        ) == 0
    }
}

// ---------------------------------------------------------------- 触发角

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreate(source: *mut c_void) -> *mut c_void;
    fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
    fn CFRelease(cf: *const c_void);
    fn CGMainDisplayID() -> u32;
    fn CGDisplayBounds(display: u32) -> CGRect;
}

pub fn set_hot_corner(corner: HotCorner) {
    let zone = match corner {
        HotCorner::None => 0,
        HotCorner::TopLeft => 1,
        HotCorner::TopRight => 2,
        HotCorner::BottomLeft => 3,
        HotCorner::BottomRight => 4,
    };
    CORNER_ZONE.store(zone, Ordering::SeqCst);
}

/// 后台轮询鼠标位置
pub fn start_corner_watcher() {
    std::thread::spawn(|| {
        let mut inside = false;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(120));
            let zone = CORNER_ZONE.load(Ordering::Relaxed);
            if zone == 0 {
                inside = false;
                continue;
            }
            let pos = unsafe {
                let ev = CGEventCreate(std::ptr::null_mut());
                if ev.is_null() {
                    continue;
                }
                let p = CGEventGetLocation(ev);
                CFRelease(ev as *const c_void);
                p
            };
            let bounds = unsafe { CGDisplayBounds(CGMainDisplayID()) };
            let (w, h) = (bounds.size.width, bounds.size.height);
            let margin = 6.0;
            let near = match zone {
                1 => pos.x < margin && pos.y < margin,
                2 => pos.x > w - margin && pos.y < margin,
                3 => pos.x < margin && pos.y > h - margin,
                4 => pos.x > w - margin && pos.y > h - margin,
                _ => false,
            };
            if near && !inside {
                inside = true;
                CORNER_EXIT.store(true, Ordering::SeqCst);
                request_repaint();
            } else if !near {
                inside = false;
            }
        }
    });
}

// ---------------------------------------------------------------- 开机自启动

pub fn launch_agent_path() -> Option<std::path::PathBuf> {
    let home = dirs::home_dir()?;
    Some(
        home.join("Library/LaunchAgents")
            .join("app.launchdeck.macos.plist"),
    )
}

/// 删除改名前的 LaunchAgent（它指向旧的可执行文件路径，留着会让登录项失效）
pub fn remove_legacy_launch_agent() {
    if let Some(home) = dirs::home_dir() {
        let old = home.join("Library/LaunchAgents/app.launchos.rust.plist");
        if old.exists() {
            let _ = std::fs::remove_file(&old);
        }
    }
}

pub fn set_launch_at_login(enable: bool) -> std::io::Result<()> {
    let path = launch_agent_path()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no home dir"))?;
    if !enable {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>app.launchdeck.macos</string>
  <key>ProgramArguments</key>
  <array><string>{}</string></array>
  <key>RunAtLoad</key><true/>
  <key>ProcessType</key><string>Interactive</string>
</dict>
</plist>
"#,
        exe.display()
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, plist)
}

// ---------------------------------------------------------------- 程序坞图标

#[link(name = "AppKit", kind = "framework")]
extern "C" {}

#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> *mut c_void;
    fn sel_registerName(name: *const c_char) -> *mut c_void;
    fn objc_msgSend();
}

/// 显示/隐藏程序坞图标（NSApplicationActivationPolicy）
pub fn set_dock_icon_visible(visible: bool) {
    unsafe {
        let cls = objc_getClass(b"NSApplication\0".as_ptr() as *const c_char);
        if cls.is_null() {
            return;
        }
        let shared: extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let app = shared(cls, sel_registerName(b"sharedApplication\0".as_ptr() as *const c_char));
        if app.is_null() {
            return;
        }
        let set_policy: extern "C" fn(*mut c_void, *mut c_void, c_int) -> bool =
            std::mem::transmute(objc_msgSend as *const ());
        // 0 = Regular（显示图标），1 = Accessory（仅菜单栏）
        let ok = set_policy(
            app,
            sel_registerName(b"setActivationPolicy:\0".as_ptr() as *const c_char),
            if visible { 0 } else { 1 },
        );
        let get_policy: extern "C" fn(*mut c_void, *mut c_void) -> i64 =
            std::mem::transmute(objc_msgSend as *const ());
        let cur = get_policy(
            app,
            sel_registerName(b"activationPolicy\0".as_ptr() as *const c_char),
        );
        if std::env::var("LAUNCHDECK_NSDEBUG").is_ok() {
            eprintln!(
                "[ns] setActivationPolicy({}) -> {ok}, now={cur}",
                if visible { 0 } else { 1 }
            );
        }
    }
}

// 避免 unused 警告
#[allow(dead_code)]
fn _keep(_: c_char) {}

// ---------------------------------------------------------------- 菜单栏图标

use std::sync::atomic::AtomicPtr;

static MENU_ACTION: AtomicU8 = AtomicU8::new(0);
static STATUS_ITEM: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static STATUS_MENU: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

pub const MENU_OPEN: u8 = 1;
pub const MENU_SETTINGS: u8 = 2;
pub const MENU_QUIT: u8 = 3;
pub const MENU_FULLSCREEN: u8 = 4;
pub const MENU_WINDOW: u8 = 5;
pub const MENU_REFRESH: u8 = 6;

pub fn take_menu_action() -> u8 {
    MENU_ACTION.swap(0, Ordering::SeqCst)
}

/// 点击程序坞图标（reopen）→ 打开启动台
unsafe extern "C" fn action_reopen(
    _this: *mut c_void,
    _cmd: *mut c_void,
    _app: *mut c_void,
    _has_visible: bool,
) -> bool {
    MENU_ACTION.store(MENU_OPEN, Ordering::SeqCst);
    request_repaint();
    true
}

/// 给 NSApplication delegate 挂上 applicationShouldHandleReopen:hasVisibleWindows:
pub fn install_reopen_handler() {
    unsafe {
        let app = msg0(cls(b"NSApplication\0"), b"sharedApplication\0");
        if app.is_null() {
            return;
        }
        let delegate = msg0(app, b"delegate\0");
        if delegate.is_null() {
            return;
        }
        let get_cls: extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let dcls = get_cls(delegate, sel(b"class\0"));
        if dcls.is_null() {
            return;
        }
        let types = b"c@:c\0".as_ptr() as *const c_char;
        class_addMethod(
            dcls,
            sel(b"applicationShouldHandleReopen:hasVisibleWindows:\0"),
            action_reopen as *const () as *const c_void,
            types,
        );
    }
}

/// 状态栏按钮被点击：左键直接打开启动台，右键弹出菜单
unsafe extern "C" fn action_status_clicked(
    _this: *mut c_void,
    _cmd: *mut c_void,
    sender: *mut c_void,
) {
    let app = msg0(cls(b"NSApplication\0"), b"sharedApplication\0");
    let ev = if app.is_null() {
        std::ptr::null_mut()
    } else {
        msg0(app, b"currentEvent\0")
    };
    let mut is_right = false;
    if !ev.is_null() {
        let t: extern "C" fn(*mut c_void, *mut c_void) -> i64 =
            std::mem::transmute(objc_msgSend as *const ());
        let ty = t(ev, sel(b"type\0"));
        // 3 = rightMouseDown，4 = rightMouseUp（不同系统/掩码下可能落到其中之一）
        is_right = ty == 3 || ty == 4;
        if std::env::var("LAUNCHDECK_NSDEBUG").is_ok() {
            eprintln!("[ns] status click event type={ty}");
        }
    }
    if is_right {
        let menu = STATUS_MENU.load(Ordering::SeqCst);
        if !menu.is_null() {
            let pop: extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            pop(
                cls(b"NSMenu\0"),
                sel(b"popUpContextMenu:withEvent:forView:\0"),
                menu,
                ev,
                sender,
            );
        }
    } else {
        MENU_ACTION.store(MENU_OPEN, Ordering::SeqCst);
        request_repaint();
    }
}

unsafe extern "C" fn action_open(
    _this: *mut c_void,
    _cmd: *mut c_void,
    _sender: *mut c_void,
) {
    MENU_ACTION.store(MENU_OPEN, Ordering::SeqCst);
    request_repaint();
}
unsafe extern "C" fn action_settings(
    _this: *mut c_void,
    _cmd: *mut c_void,
    _sender: *mut c_void,
) {
    MENU_ACTION.store(MENU_SETTINGS, Ordering::SeqCst);
    request_repaint();
}
unsafe extern "C" fn action_fullscreen(
    _this: *mut c_void,
    _cmd: *mut c_void,
    _sender: *mut c_void,
) {
    MENU_ACTION.store(MENU_FULLSCREEN, Ordering::SeqCst);
    request_repaint();
}
unsafe extern "C" fn action_window(
    _this: *mut c_void,
    _cmd: *mut c_void,
    _sender: *mut c_void,
) {
    MENU_ACTION.store(MENU_WINDOW, Ordering::SeqCst);
    request_repaint();
}
unsafe extern "C" fn action_refresh(
    _this: *mut c_void,
    _cmd: *mut c_void,
    _sender: *mut c_void,
) {
    MENU_ACTION.store(MENU_REFRESH, Ordering::SeqCst);
    request_repaint();
}
unsafe extern "C" fn action_quit(
    _this: *mut c_void,
    _cmd: *mut c_void,
    _sender: *mut c_void,
) {
    MENU_ACTION.store(MENU_QUIT, Ordering::SeqCst);
    request_repaint();
}

#[link(name = "objc")]
extern "C" {
    fn objc_allocateClassPair(
        superclass: *mut c_void,
        name: *const c_char,
        extra_bytes: usize,
    ) -> *mut c_void;
    fn objc_registerClassPair(cls: *mut c_void);
    fn class_addMethod(
        cls: *mut c_void,
        sel: *mut c_void,
        imp: *const c_void,
        types: *const c_char,
    ) -> bool;
}

unsafe fn cls(name: &[u8]) -> *mut c_void {
    objc_getClass(name.as_ptr() as *const c_char)
}

unsafe fn sel(name: &[u8]) -> *mut c_void {
    sel_registerName(name.as_ptr() as *const c_char)
}

unsafe fn msg0(obj: *mut c_void, s: &[u8]) -> *mut c_void {
    let f: extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
        std::mem::transmute(objc_msgSend as *const ());
    f(obj, sel(s))
}

unsafe fn msg1(obj: *mut c_void, s: &[u8], a: *mut c_void) -> *mut c_void {
    let f: extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> *mut c_void =
        std::mem::transmute(objc_msgSend as *const ());
    f(obj, sel(s), a)
}

unsafe fn msg_void1(obj: *mut c_void, s: &[u8], a: *mut c_void) {
    let f: extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
        std::mem::transmute(objc_msgSend as *const ());
    f(obj, sel(s), a)
}

unsafe fn nsstring(text: &str) -> *mut c_void {
    let f: extern "C" fn(*mut c_void, *mut c_void, *const c_char) -> *mut c_void =
        std::mem::transmute(objc_msgSend as *const ());
    let c = std::ffi::CString::new(text).unwrap_or_default();
    f(cls(b"NSString\0"), sel(b"stringWithUTF8String:\0"), c.as_ptr())
}

unsafe fn menu_item(title: &str, action: &[u8]) -> *mut c_void {
    let f: extern "C" fn(
        *mut c_void,
        *mut c_void,
        *mut c_void,
        *mut c_void,
        *mut c_void,
    ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
    let alloc = msg0(cls(b"NSMenuItem\0"), b"alloc\0");
    f(
        alloc,
        sel(b"initWithTitle:action:keyEquivalent:\0"),
        nsstring(title),
        sel(action),
        nsstring(""),
    )
}

/// 安装菜单栏图标。返回是否成功。
pub fn install_menu_bar() -> bool {
    unsafe {
        if !STATUS_ITEM.load(Ordering::SeqCst).is_null() {
            return true;
        }
        // 动态创建点击目标类；第二次安装时类已存在，直接复用
        let superclass = cls(b"NSObject\0");
        let mut target_cls =
            objc_allocateClassPair(superclass, b"LaunchDeckMenuTarget\0".as_ptr() as *const c_char, 0);
        if target_cls.is_null() {
            target_cls = objc_getClass(b"LaunchDeckMenuTarget\0".as_ptr() as *const c_char);
        }
        if target_cls.is_null() {
            return false;
        }
        let types = b"v@:@\0".as_ptr() as *const c_char;
        class_addMethod(target_cls, sel(b"openLauncher:\0"), action_open as *const () as *const c_void, types);
        class_addMethod(target_cls, sel(b"openSettings:\0"), action_settings as *const () as *const c_void, types);
        class_addMethod(target_cls, sel(b"fullscreen:\0"), action_fullscreen as *const () as *const c_void, types);
        class_addMethod(target_cls, sel(b"windowMode:\0"), action_window as *const () as *const c_void, types);
        class_addMethod(target_cls, sel(b"refreshApps:\0"), action_refresh as *const () as *const c_void, types);
        class_addMethod(target_cls, sel(b"quitApp:\0"), action_quit as *const () as *const c_void, types);
        class_addMethod(target_cls, sel(b"statusClicked:\0"), action_status_clicked as *const () as *const c_void, types);
        objc_registerClassPair(target_cls);

        let target = msg0(msg0(target_cls, b"alloc\0"), b"init\0");
        if target.is_null() {
            return false;
        }
        msg0(target, b"retain\0");

        let bar = msg0(cls(b"NSStatusBar\0"), b"systemStatusBar\0");
        let f: extern "C" fn(*mut c_void, *mut c_void, f64) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let item = f(bar, sel(b"statusItemWithLength:\0"), -1.0);
        if item.is_null() {
            return false;
        }
        msg0(item, b"retain\0");
        STATUS_ITEM.store(item, Ordering::SeqCst);

        // 菜单栏图标：系统符号渲染的模板图（随明暗自动反色），不用 emoji
        let button = msg0(item, b"button\0");
        let img_cls = cls(b"NSImage\0");
        let sym_name = nsstring("square.grid.2x2");
        let f2: extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let img = f2(
            img_cls,
            sel(b"imageWithSystemSymbolName:accessibilityDescription:\0"),
            sym_name,
            std::ptr::null_mut(),
        );
        if !img.is_null() {
            let set_bool: extern "C" fn(*mut c_void, *mut c_void, bool) =
                std::mem::transmute(objc_msgSend as *const ());
            set_bool(img, sel(b"setTemplate:\0"), true);
            let set_size: extern "C" fn(*mut c_void, *mut c_void, f64, f64) =
                std::mem::transmute(objc_msgSend as *const ());
            set_size(img, sel(b"setSize:\0"), 17.0, 17.0);
            msg_void1(button, b"setImage:\0", img);
        }
        msg_void1(button, b"setTitle:\0", nsstring(""));

        // 菜单
        let menu = msg0(msg0(cls(b"NSMenu\0"), b"alloc\0"), b"init\0");
        msg1(menu, b"setAutoenablesItems:\0", std::ptr::null_mut());

        for (title, action) in [
            ("打开启动台", b"openLauncher:\0".as_slice()),
            ("设置…", b"openSettings:\0".as_slice()),
        ] {
            let it = menu_item(title, action);
            msg1(it, b"setTarget:\0", target);
            msg_void1(menu, b"addItem:\0", it);
        }
        let sep = msg0(cls(b"NSMenuItem\0"), b"separatorItem\0");
        msg_void1(menu, b"addItem:\0", sep);
        for (title, action) in [
            ("全屏模式", b"fullscreen:\0".as_slice()),
            ("窗口模式", b"windowMode:\0".as_slice()),
            ("刷新应用", b"refreshApps:\0".as_slice()),
        ] {
            let it = menu_item(title, action);
            msg1(it, b"setTarget:\0", target);
            msg_void1(menu, b"addItem:\0", it);
        }
        let sep = msg0(cls(b"NSMenuItem\0"), b"separatorItem\0");
        msg_void1(menu, b"addItem:\0", sep);
        let quit = menu_item("退出 LaunchDeck", b"quitApp:\0");
        msg1(quit, b"setTarget:\0", target);
        msg_void1(menu, b"addItem:\0", quit);

        // 左键：直接打开启动台；右键：弹出菜单
        STATUS_MENU.store(menu, Ordering::SeqCst);
        msg1(button, b"setTarget:\0", target);
        msg1(button, b"setAction:\0", sel(b"statusClicked:\0"));
        let send_on: extern "C" fn(*mut c_void, *mut c_void, u64) -> u64 =
            std::mem::transmute(objc_msgSend as *const ());
        // NSEventMaskLeftMouseUp(1<<1) | NSEventMaskRightMouseUp(1<<3)
        send_on(button, sel(b"sendActionOn:\0"), (1 << 1) | (1 << 3));
        true
    }
}

/// 移除菜单栏图标
pub fn remove_menu_bar() {
    unsafe {
        let item = STATUS_ITEM.swap(std::ptr::null_mut(), Ordering::SeqCst);
        if !item.is_null() {
            let bar = msg0(cls(b"NSStatusBar\0"), b"systemStatusBar\0");
            msg_void1(bar, b"removeStatusItem:\0", item);
        }
    }
}

// ---------------------------------------------------------------- 壁纸路径

unsafe fn cfstring_to_string(ns: *mut c_void) -> Option<String> {
    if ns.is_null() {
        return None;
    }
    let f: extern "C" fn(*mut c_void, *mut c_void) -> *const c_char =
        std::mem::transmute(objc_msgSend as *const ());
    let p = f(ns, sel(b"UTF8String\0"));
    if p.is_null() {
        None
    } else {
        Some(std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned())
    }
}

/// 当前主屏壁纸的本地路径
pub fn wallpaper_path() -> Option<String> {
    unsafe {
        let workspace = msg0(cls(b"NSWorkspace\0"), b"sharedWorkspace\0");
        if workspace.is_null() {
            return None;
        }
        let screen = msg0(cls(b"NSScreen\0"), b"mainScreen\0");
        let url = msg1(workspace, b"desktopImageURLForScreen:\0", screen);
        if url.is_null() {
            return None;
        }
        let path = msg0(url, b"path\0");
        cfstring_to_string(path)
    }
}

/// 全屏时隐藏菜单栏（保留 Dock）
/// 内置图标（直接运行裸二进制、没有 Resources 时的兜底）
static EMBEDDED_ICONS: [&[u8]; 4] = [
    include_bytes!("../../assets/iconstyles/embed/style0.png"),
    include_bytes!("../../assets/iconstyles/embed/style1.png"),
    include_bytes!("../../assets/iconstyles/embed/style2.png"),
    include_bytes!("../../assets/iconstyles/embed/style3.png"),
];

/// 用字节构造 NSImage
unsafe fn image_from_bytes(bytes: &[u8]) -> *mut c_void {
    let data_cls = cls(b"NSData\0");
    let f: extern "C" fn(*mut c_void, *mut c_void, *const c_void, u64) -> *mut c_void =
        std::mem::transmute(objc_msgSend as *const ());
    let data = f(
        data_cls,
        sel(b"dataWithBytes:length:\0"),
        bytes.as_ptr() as *const c_void,
        bytes.len() as u64,
    );
    if data.is_null() {
        return std::ptr::null_mut();
    }
    let alloc = msg0(cls(b"NSImage\0"), b"alloc\0");
    if alloc.is_null() {
        return std::ptr::null_mut();
    }
    msg1(alloc, b"initWithData:\0", data)
}

/// 按「应用图标」设置切换 App 自己的图标（Dock / 程序坞）
pub fn set_app_icon(style: usize) {
    unsafe {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let Some(res) = exe.parent().and_then(|p| p.parent()) else {
            return;
        };
        let path = res
            .join("Resources")
            .join("iconstyles")
            .join(format!("style{style}.png"));
        let img = if path.exists() {
            let alloc = msg0(cls(b"NSImage\0"), b"alloc\0");
            if alloc.is_null() {
                return;
            }
            msg1(alloc, b"initWithContentsOfFile:\0", nsstring(&path.to_string_lossy()))
        } else {
            // 裸二进制运行时没有 Resources，用编译进程序里的图标
            image_from_bytes(EMBEDDED_ICONS[style.min(3)])
        };
        if std::env::var("LAUNCHDECK_NSDEBUG").is_ok() {
            eprintln!(
                "[ns] set_app_icon style={style} path_exists={} img_null={}",
                path.exists(),
                img.is_null()
            );
        }
        if img.is_null() {
            return;
        }
        let app = msg0(cls(b"NSApplication\0"), b"sharedApplication\0");
        if app.is_null() {
            return;
        }
        msg_void1(app, b"setApplicationIconImage:\0", img);
        // 菜单栏图标也跟随（否则隐藏了 Dock 图标时看不出变化）
        let item = STATUS_ITEM.load(Ordering::SeqCst);
        if !item.is_null() {
            let set_t: extern "C" fn(*mut c_void, *mut c_void, bool) =
                std::mem::transmute(objc_msgSend as *const ());
            set_t(img, sel(b"setTemplate:\0"), false);
            let set_size: extern "C" fn(*mut c_void, *mut c_void, f64, f64) =
                std::mem::transmute(objc_msgSend as *const ());
            set_size(img, sel(b"setSize:\0"), 18.0, 18.0);
            let button = msg0(item, b"button\0");
            msg_void1(button, b"setImage:\0", img);
        }
    }
}

/// 用 NSRunningApplication 激活自己（Accessory 应用用 activateIgnoringOtherApps 常常无效）
pub fn activate_self() {
    unsafe {
        let cls = cls(b"NSRunningApplication\0");
        if cls.is_null() {
            return;
        }
        let cur = msg0(cls, b"currentApplication\0");
        if cur.is_null() {
            return;
        }
        // NSApplicationActivateAllWindows(1) | NSApplicationActivateIgnoringOtherApps(2)
        let f: extern "C" fn(*mut c_void, *mut c_void, u64) -> bool =
            std::mem::transmute(objc_msgSend as *const ());
        let ok = f(cur, sel(b"activateWithOptions:\0"), 3);
        if std::env::var("LAUNCHDECK_NSDEBUG").is_ok() {
            eprintln!("[ns] activateWithOptions -> {ok}");
        }
    }
}

/// 让 App 成为前台应用（热键唤出时否则菜单栏仍是别的 App 的）
pub fn activate_app() {
    unsafe {
        let app = msg0(cls(b"NSApplication\0"), b"sharedApplication\0");
        if app.is_null() {
            return;
        }
        let f: extern "C" fn(*mut c_void, *mut c_void, bool) =
            std::mem::transmute(objc_msgSend as *const ());
        f(app, sel(b"activateIgnoringOtherApps:\0"), true);
    }
}

/// 当前最前面的 App 是否是我们（用 NSWorkspace 判断，比 isActive 更贴近用户感知）
pub fn frontmost_is_self() -> bool {
    unsafe {
        let ws = msg0(cls(b"NSWorkspace\0"), b"sharedWorkspace\0");
        if ws.is_null() {
            return true;
        }
        let app = msg0(ws, b"frontmostApplication\0");
        if app.is_null() {
            return true;
        }
        let f: extern "C" fn(*mut c_void, *mut c_void) -> i32 =
            std::mem::transmute(objc_msgSend as *const ());
        f(app, sel(b"processIdentifier\0")) == std::process::id() as i32
    }
}

/// App 是否处于前台（比窗口 focused 更可靠，热键唤出时窗口常晚一步变 key）
pub fn app_is_active() -> bool {
    unsafe {
        let app = msg0(cls(b"NSApplication\0"), b"sharedApplication\0");
        if app.is_null() {
            return false;
        }
        let f: extern "C" fn(*mut c_void, *mut c_void) -> bool =
            std::mem::transmute(objc_msgSend as *const ());
        f(app, sel(b"isActive\0"))
    }
}

/// 设置主窗口层级。25 = 覆盖菜单栏（24），实现真正的全屏
/// 如果已有同名 App 在运行，返回它的 pid（用于单实例保护）
pub fn other_instance_pid() -> Option<i32> {
    unsafe {
        let cls_ra = cls(b"NSRunningApplication\0");
        if cls_ra.is_null() {
            return None;
        }
        let arr = msg1(
            cls_ra,
            b"runningApplicationsWithBundleIdentifier:\0",
            nsstring("app.launchdeck.macos"),
        );
        if arr.is_null() {
            return None;
        }
        let count_fn: extern "C" fn(*mut c_void, *mut c_void) -> usize =
            std::mem::transmute(objc_msgSend as *const ());
        let n = count_fn(arr, sel(b"count\0"));
        if n <= 1 {
            return None;
        }
        let idx_fn: extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let pid_fn: extern "C" fn(*mut c_void, *mut c_void) -> i32 =
            std::mem::transmute(objc_msgSend as *const ());
        let self_pid = std::process::id() as i32;
        for i in 0..n {
            let a = idx_fn(arr, sel(b"objectAtIndex:\0"), i);
            if a.is_null() {
                continue;
            }
            let p = pid_fn(a, sel(b"processIdentifier\0"));
            if p != self_pid {
                return Some(p);
            }
        }
        None
    }
}

#[repr(C)]
pub struct NSPoint { pub x: f64, pub y: f64 }
#[repr(C)]
pub struct NSSize { pub width: f64, pub height: f64 }
#[repr(C)]
pub struct NSRect { pub origin: NSPoint, pub size: NSSize }

/// 主屏尺寸（点），用于创建窗口时一次到位，避免启动时从 1024×700 跳到全屏
pub fn main_screen_size() -> Option<(f64, f64)> {
    unsafe {
        let scr = msg0(cls(b"NSScreen\0"), b"mainScreen\0");
        if scr.is_null() {
            return None;
        }
        let f: extern "C" fn(*mut c_void, *mut c_void) -> NSRect =
            std::mem::transmute(objc_msgSend as *const ());
        let r = f(scr, sel(b"frame\0"));
        if r.size.width > 0.0 && r.size.height > 0.0 {
            Some((r.size.width, r.size.height))
        } else {
            None
        }
    }
}

pub fn set_window_level(level: i64) {
    unsafe {
        let app = msg0(cls(b"NSApplication\0"), b"sharedApplication\0");
        if app.is_null() {
            return;
        }
        let mut win = msg0(app, b"mainWindow\0");
        if win.is_null() {
            // App 非激活时 mainWindow 可能为空，退回 windows[0]
            let arr = msg0(app, b"windows\0");
            if arr.is_null() {
                return;
            }
            let f: extern "C" fn(*mut c_void, *mut c_void) -> usize =
                std::mem::transmute(objc_msgSend as *const ());
            if f(arr, sel(b"count\0")) > 0 {
                win = msg1(arr, b"objectAtIndex:\0", std::ptr::null_mut());
            }
            if win.is_null() {
                return;
            }
        }
        if std::env::var("LAUNCHDECK_NSDEBUG").is_ok() {
            eprintln!("[ns] set_window_level({level}) win={win:p}");
        }
        let f: extern "C" fn(*mut c_void, *mut c_void, i64) =
            std::mem::transmute(objc_msgSend as *const ());
        f(win, sel(b"setLevel:\0"), level);
        if std::env::var("LAUNCHDECK_NSDEBUG").is_ok() {
            eprintln!("[ns] set_window_level({level}) done");
        }
    }
}

pub fn set_menu_bar_hidden(hidden: bool) {
    unsafe {
        let app = msg0(cls(b"NSApplication\0"), b"sharedApplication\0");
        if app.is_null() {
            return;
        }
        let f: extern "C" fn(*mut c_void, *mut c_void, u64) =
            std::mem::transmute(objc_msgSend as *const ());
        // HideMenuBar 必须与 HideDock 同时使用，否则组合非法会被系统忽略
        // NSApplicationPresentationHideMenuBar = 1 << 1
        // NSApplicationPresentationHideDock    = 1 << 2
        f(
            app,
            sel(b"setPresentationOptions:\0"),
            if hidden { (1 << 1) | (1 << 2) } else { 0 },
        );
        let get: extern "C" fn(*mut c_void, *mut c_void) -> u64 =
            std::mem::transmute(objc_msgSend as *const ());
        let now = get(app, sel(b"presentationOptions\0"));
        if std::env::var("LAUNCHDECK_NSDEBUG").is_ok() {
            eprintln!("[ns] set_menu_bar_hidden({hidden}) -> presentationOptions=0x{now:x}");
        }
    }
}

/// 强制 App 使用深色外观（原生菜单、标题栏随之变暗）
pub fn force_dark_appearance() {
    unsafe {
        let appearance_cls = cls(b"NSAppearance\0");
        if appearance_cls.is_null() {
            return;
        }
        let name = nsstring("NSAppearanceNameDarkAqua");
        let appearance = msg1(appearance_cls, b"appearanceNamed:\0", name);
        if appearance.is_null() {
            return;
        }
        let app = msg0(cls(b"NSApplication\0"), b"sharedApplication\0");
        if !app.is_null() {
            msg1(app, b"setAppearance:\0", appearance);
        }
    }
}


/// 注册 F4 为打开启动台（Carbon 热键不需要辅助功能权限）
pub fn register_f4() -> bool {
    unsafe {
        let target = GetApplicationEventTarget();
        let hot_id = EventHotKeyID {
            signature: 0x4C_4F_53_32, // 'LOS2'
            id: 2,
        };
        let mut hk_ref: *mut c_void = std::ptr::null_mut();
        let status = RegisterEventHotKey(118, 0, hot_id, target, 0, &mut hk_ref);
        if std::env::var("LAUNCHDECK_NSDEBUG").is_ok() {
            eprintln!("[ns] register F4 -> status={status}");
        }
        let ok = status == 0;
        F4_OK.store(ok, Ordering::SeqCst);
        ok
    }
}

pub fn f4_ok() -> bool {
    F4_OK.load(Ordering::SeqCst)
}

pub fn take_f4() -> bool {
    F4_PRESSED.swap(false, Ordering::SeqCst)
}

/// 触控板横扫：全局监听滚轮事件的横向分量（需要辅助功能权限）
pub fn install_swipe_tap() -> bool {
    unsafe {
        let mask = 1u64 << 22; // kCGEventScrollWheel
        let tap = CGEventTapCreate(
            1, // kCGSessionEventTap
            0, // kCGHeadInsertEventTap
            1, // kCGEventTapOptionListenOnly
            mask,
            Some(swipe_callback),
            std::ptr::null_mut(),
        );
        if std::env::var("LAUNCHDECK_NSDEBUG").is_ok() {
            eprintln!("[ns] swipe tap: {}", if tap.is_null() { "null (无辅助功能权限)" } else { "ok" });
        }
        if tap.is_null() {
            SWIPE_OK.store(false, Ordering::SeqCst);
            return false;
        }
        let src = CFMachPortCreateRunLoopSource(std::ptr::null_mut(), tap, 0);
        if src.is_null() {
            return false;
        }
        CFRunLoopAddSource(CFRunLoopGetCurrent(), src, kCFRunLoopCommonModes as *mut c_void);
        SWIPE_OK.store(true, Ordering::SeqCst);
        true
    }
}

pub fn swipe_ok() -> bool {
    SWIPE_OK.load(Ordering::SeqCst)
}

/// 取出一次横扫方向：-1 上一页，1 下一页
pub fn take_swipe() -> i32 {
    SWIPE_DIR.swap(0, Ordering::SeqCst)
}

unsafe extern "C" fn swipe_callback(
    _proxy: *mut c_void,
    _type: u32,
    event: *mut c_void,
    _user: *mut c_void,
) -> *mut c_void {
    // 系统可能发来 tap 被禁用等事件（event 为空或类型不符），必须挡掉
    if event.is_null() || _type != 22 {
        return event;
    }
    let mut dx = CGEventGetDoubleValueField(event, 11); // kCGScrollWheelEventDeltaAxis2（行）
    if dx.abs() < 0.01 {
        // 像素级滚动事件用 PointDelta（12），换算成相近的尺度
        dx = CGEventGetDoubleValueField(event, 12) * 0.1;
    }
    if dx.abs() < 0.01 {
        SWIPE_ACC.store(0, Ordering::SeqCst);
        return event;
    }
    let acc = SWIPE_ACC.load(Ordering::SeqCst) as f64 / 10.0 + dx;
    if acc.abs() > 22.0 {
        SWIPE_DIR.store(if acc > 0.0 { -1 } else { 1 }, Ordering::SeqCst);
        SWIPE_ACC.store(0, Ordering::SeqCst);
        request_repaint();
    } else {
        SWIPE_ACC.store((acc * 10.0) as i32, Ordering::SeqCst);
    }
    event
}

// ---------------------------------------------------------------- 触控板捏合手势

// NSEvent 全局监听（公开 API，无需辅助功能权限）。
// ObjC block 采用手搭的全局 block（_NSConcreteGlobalBlock，不捕获上下文）。
static PINCH_ACC: AtomicI32 = AtomicI32::new(0); // 累计放大倍数 ×1000
static PINCH_DIR: AtomicI32 = AtomicI32::new(0); // 1=捏合(打开) -1=张开(关闭)
static PINCH_FINGERS: AtomicU8 = AtomicU8::new(0); // 最少手指数，0=关闭手势
static PINCH_LAST: AtomicI64 = AtomicI64::new(0);

#[repr(C)]
struct ObjCBlock {
    isa: *const c_void,
    flags: c_int,
    reserved: c_int,
    invoke: unsafe extern "C" fn(*const ObjCBlock, *mut c_void),
    descriptor: *const c_void,
}

#[repr(C)]
struct ObjCBlockDescriptor {
    reserved: usize,
    size: usize,
}

static PINCH_BLOCK_DESC: ObjCBlockDescriptor = ObjCBlockDescriptor {
    reserved: 0,
    size: std::mem::size_of::<ObjCBlock>(),
};

extern "C" {
    static _NSConcreteGlobalBlock: c_void;
}

static PINCH_BLOCK: ObjCBlock = ObjCBlock {
    isa: unsafe { &_NSConcreteGlobalBlock },
    flags: 0,
    reserved: 0,
    invoke: pinch_block_invoke,
    descriptor: &PINCH_BLOCK_DESC as *const ObjCBlockDescriptor as *const c_void,
};

// 全局 block（_NSConcreteGlobalBlock）初始化后不可变，跨线程共享是安全的
unsafe impl Sync for ObjCBlock {}

unsafe extern "C" fn pinch_block_invoke(_b: *const ObjCBlock, event: *mut c_void) {
    if event.is_null() {
        return;
    }
    let min = PINCH_FINGERS.load(Ordering::SeqCst) as usize;
    if min == 0 {
        return;
    }
    // 手指数不满足设置时忽略
    let touches = msg0(event, b"allTouches\0");
    let n = if touches.is_null() { 0 } else { msg_usize(touches, b"count\0") };
    if n > 0 && n < min {
        return;
    }
    let mag = msg_f64(event, b"magnification\0");
    // 1.5s 内没有后续事件则重新累计
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let last = PINCH_LAST.swap(now, Ordering::SeqCst);
    if now - last > 1500 {
        PINCH_ACC.store(0, Ordering::SeqCst);
    }
    let acc = PINCH_ACC.load(Ordering::SeqCst) as f64 / 1000.0 + mag;
    if acc < -0.55 {
        PINCH_DIR.store(1, Ordering::SeqCst); // 捏合 → 打开
        PINCH_ACC.store(0, Ordering::SeqCst);
        request_repaint();
    } else if acc > 0.55 {
        PINCH_DIR.store(-1, Ordering::SeqCst); // 张开 → 关闭
        PINCH_ACC.store(0, Ordering::SeqCst);
        request_repaint();
    } else {
        PINCH_ACC.store((acc * 1000.0) as i32, Ordering::SeqCst);
    }
}

/// 安装捏合手势全局监听（NSEventTypeMagnify = 1 << 30）
pub fn install_pinch_monitor() -> bool {
    unsafe {
        let f: extern "C" fn(*mut c_void, *mut c_void, u64, *const c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let mon = f(
            cls(b"NSEvent\0"),
            sel(b"addGlobalMonitorForEventsMatchingMask:handler:\0"),
            1u64 << 30,
            &PINCH_BLOCK as *const ObjCBlock as *const c_void,
        );
        let ok = !mon.is_null();
        if std::env::var("LAUNCHDECK_NSDEBUG").is_ok() {
            eprintln!("[ns] pinch monitor: {}", if ok { "ok" } else { "null" });
        }
        ok
    }
}

/// 手指数阈值：0 关闭，3/4/5 对应设置项（四指或五指 = 4）
pub fn set_pinch_fingers(min: u8) {
    PINCH_FINGERS.store(min, Ordering::SeqCst);
}

/// 取出一次捏合动作：1=捏合打开，-1=张开关闭，0=无
pub fn take_pinch() -> i32 {
    PINCH_DIR.swap(0, Ordering::SeqCst)
}

// ---------------------------------------------------------------- 音效

/// 播放移入废纸篓音效（系统无 trash 命名音时退回 Pop）
pub fn play_trash_sound() {
    unsafe {
        let mut snd = msg1(cls(b"NSSound\0"), b"soundNamed:\0", nsstring("dragToTrash"));
        if snd.is_null() {
            snd = msg1(cls(b"NSSound\0"), b"soundNamed:\0", nsstring("Pop"));
        }
        if !snd.is_null() {
            let _ = msg0(snd, b"play\0");
        }
    }
}

// ---------------------------------------------------------------- 多屏

// NSPoint / NSSize / NSRect 复用上文 951-957 行的既有定义。

unsafe fn msg_f64(obj: *mut c_void, s: &[u8]) -> f64 {
    let f: extern "C" fn(*mut c_void, *mut c_void) -> f64 =
        std::mem::transmute(objc_msgSend as *const ());
    f(obj, sel(s))
}

unsafe fn msg_usize(obj: *mut c_void, s: &[u8]) -> usize {
    let f: extern "C" fn(*mut c_void, *mut c_void) -> usize =
        std::mem::transmute(objc_msgSend as *const ());
    f(obj, sel(s))
}

// NSRect(32B) 走隐藏 sret 指针；NSPoint(16B) 直接经 x0/x1 返回。
// arm64 的 objc_msgSend 对结构体返回无需 _stret 变体。
unsafe fn msg_rect(obj: *mut c_void, s: &[u8]) -> NSRect {
    let f: extern "C" fn(*mut c_void, *mut c_void) -> NSRect =
        std::mem::transmute(objc_msgSend as *const ());
    f(obj, sel(s))
}

unsafe fn msg_point(obj: *mut c_void, s: &[u8]) -> NSPoint {
    let f: extern "C" fn(*mut c_void, *mut c_void) -> NSPoint =
        std::mem::transmute(objc_msgSend as *const ());
    f(obj, sel(s))
}

type CFArrayRef = *const c_void;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFArrayGetCount(a: CFArrayRef) -> isize;
    fn CFArrayGetValueAtIndex(a: CFArrayRef, i: isize) -> *const c_void;
    fn CFDictionaryGetValue(d: *const c_void, key: *const c_void) -> *const c_void;
    fn CFNumberGetValue(n: *const c_void, ntype: isize, v: *mut c_void) -> bool;
    fn CFStringCreateWithCString(alloc: *const c_void, s: *const c_char, enc: u32) -> *const c_void;
    static kCGWindowLayer: *const c_void;
    static kCGWindowBounds: *const c_void;
    static kCGWindowOwnerPID: *const c_void;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative: u32) -> CFArrayRef;
}

extern "C" {
    fn getpid() -> i32;
}

unsafe fn cfstr(s: &str) -> *const c_void {
    let c = std::ffi::CString::new(s).unwrap_or_default();
    CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), 0x0800_0100) // kCFStringEncodingUTF8
}

/// 读取 CGWindow 字典的 bounds（已是左上角原点的 CG 坐标）
unsafe fn window_bounds(dict: *const c_void) -> Option<(f64, f64, f64, f64)> {
    let b = CFDictionaryGetValue(dict, kCGWindowBounds);
    if b.is_null() {
        return None;
    }
    let num = |key: &str| -> f64 {
        let k = cfstr(key);
        let v = CFDictionaryGetValue(b, k);
        let mut out: f64 = 0.0;
        if !v.is_null() {
            CFNumberGetValue(v, 6, &mut out as *mut f64 as *mut c_void); // kCFNumberFloat64Type
        }
        out
    };
    Some((num("X"), num("Y"), num("Width"), num("Height")))
}

unsafe fn window_layer(dict: *const c_void) -> f64 {
    let v = CFDictionaryGetValue(dict, kCGWindowLayer);
    let mut out: f64 = 0.0;
    if !v.is_null() {
        CFNumberGetValue(v, 6, &mut out as *mut f64 as *mut c_void);
    }
    out
}

unsafe fn window_pid(dict: *const c_void) -> i32 {
    let v = CFDictionaryGetValue(dict, kCGWindowOwnerPID);
    let mut out: i32 = 0;
    if !v.is_null() {
        CFNumberGetValue(v, 3, &mut out as *mut i32 as *mut c_void); // kCFNumberSInt32Type
    }
    out
}

/// 目标屏幕的 frame（x/y 为左上角原点，单位：点）。查询失败返回 None，
/// 由调用方退回当前显示器。
pub fn screen_frame(choice: ScreenChoice) -> Option<(f64, f64, f64, f64)> {
    unsafe {
        let screens = msg0(cls(b"NSScreen\0"), b"screens\0");
        if screens.is_null() {
            return None;
        }
        let count = msg_usize(screens, b"count\0");
        // AppKit 全局坐标（主屏左下角为原点，y 向上）
        let mut frames: Vec<NSRect> = Vec::new();
        for i in 0..count {
            let s = msg1(screens, b"objectAtIndex:\0", i as *mut c_void);
            frames.push(msg_rect(s, b"frame\0"));
        }
        // 主屏高度（原点 (0,0) 的那块）用于 AppKit → CG 翻转
        let primary_h = frames
            .iter()
            .find(|f| f.origin.x == 0.0 && f.origin.y == 0.0)
            .map(|f| f.size.height)
            .unwrap_or(0.0);
        let to_cg = |r: &NSRect| {
            (
                r.origin.x,
                primary_h - r.origin.y - r.size.height,
                r.size.width,
                r.size.height,
            )
        };
        let cg_frames: Vec<(f64, f64, f64, f64)> = frames.iter().map(to_cg).collect();
        let contains_cg = |f: &(f64, f64, f64, f64), px: f64, py: f64| {
            px >= f.0 && px < f.0 + f.2 && py >= f.1 && py < f.1 + f.3
        };

        match choice {
            ScreenChoice::Main => {
                let main = msg0(cls(b"NSScreen\0"), b"mainScreen\0");
                if main.is_null() {
                    return cg_frames.first().copied();
                }
                Some(to_cg(&msg_rect(main, b"frame\0")))
            }
            ScreenChoice::Cursor => {
                let mp = msg_point(cls(b"NSEvent\0"), b"mouseLocation\0");
                cg_frames
                    .iter()
                    .find(|f| {
                        // 鼠标位置是 AppKit 坐标，先转成 CG
                        let cgx = mp.x;
                        let cgy = primary_h - mp.y;
                        contains_cg(f, cgx, cgy)
                    })
                    .copied()
            }
            ScreenChoice::Dock => {
                // Dock 自己的窗口：所有者叫 Dock，取面积最大的那块
                let list = CGWindowListCopyWindowInfo(1, 0); // kCGWindowListOptionOnScreenOnly
                if list.is_null() {
                    return cg_frames.first().copied();
                }
                let n = CFArrayGetCount(list);
                let mut best: Option<(f64, f64, f64, f64)> = None;
                let mut best_area = 0.0;
                for i in 0..n {
                    let d = CFArrayGetValueAtIndex(list, i);
                    // owner name 会被系统本地化（中文是「程序坞」），改按窗口层级识别
                    if window_layer(d) != 20.0 {
                        continue;
                    }
                    if let Some((x, y, w, h)) = window_bounds(d) {
                        let area = w * h;
                        if area > best_area {
                            best_area = area;
                            best = Some((x, y, w, h));
                        }
                    }
                }
                CFRelease(list);
                let (x, y, _, _) = best?;
                cg_frames
                    .iter()
                    .find(|f| contains_cg(f, x, y))
                    .copied()
            }
            ScreenChoice::Active => {
                // 从前往后第一块 layer 0、非本进程的普通窗口
                let list = CGWindowListCopyWindowInfo(1, 0);
                if list.is_null() {
                    return None;
                }
                let n = CFArrayGetCount(list);
                let me = getpid();
                let mut found: Option<(f64, f64, f64, f64)> = None;
                for i in 0..n {
                    let d = CFArrayGetValueAtIndex(list, i);
                    if window_layer(d) != 0.0 || window_pid(d) == me {
                        continue;
                    }
                    if let Some(b) = window_bounds(d) {
                        if b.2 > 100.0 && b.3 > 100.0 {
                            found = Some(b);
                            break;
                        }
                    }
                }
                CFRelease(list);
                let (x, y, _, _) = found?;
                cg_frames
                    .iter()
                    .find(|f| contains_cg(f, x, y))
                    .copied()
            }
        }
    }
}

// ---------------------------------------------------------------- 运行检测 / 退出

/// 是否有正在运行的应用位于指定 bundle 路径
pub fn app_running_at(path: &std::path::Path) -> bool {
    unsafe {
        let ws = msg0(cls(b"NSWorkspace\0"), b"sharedWorkspace\0");
        if ws.is_null() {
            return false;
        }
        let apps = msg0(ws, b"runningApplications\0");
        if apps.is_null() {
            return false;
        }
        let want = path.to_string_lossy().trim_end_matches('/').to_string();
        let n = msg_usize(apps, b"count\0");
        for i in 0..n {
            let a = msg1(apps, b"objectAtIndex:\0", i as *mut c_void);
            let url = msg0(a, b"bundleURL\0");
            if url.is_null() {
                continue;
            }
            let p = msg0(url, b"path\0");
            if p.is_null() {
                continue;
            }
            if let Some(s) = cfstring_to_string(p) {
                if s.trim_end_matches('/') == want {
                    return true;
                }
            }
        }
        false
    }
}

/// 请求退出指定 bundle 路径的应用；force=true 时强制退出
pub fn terminate_app_at(path: &std::path::Path, force: bool) {
    unsafe {
        let ws = msg0(cls(b"NSWorkspace\0"), b"sharedWorkspace\0");
        if ws.is_null() {
            return;
        }
        let apps = msg0(ws, b"runningApplications\0");
        if apps.is_null() {
            return;
        }
        let want = path.to_string_lossy().trim_end_matches('/').to_string();
        let n = msg_usize(apps, b"count\0");
        for i in 0..n {
            let a = msg1(apps, b"objectAtIndex:\0", i as *mut c_void);
            let url = msg0(a, b"bundleURL\0");
            if url.is_null() {
                continue;
            }
            let p = msg0(url, b"path\0");
            if p.is_null() {
                continue;
            }
                if let Some(s) = cfstring_to_string(p) {
                    if s.trim_end_matches('/') == want {
                        let action: &[u8] = if force {
                            b"forceTerminate\0"
                        } else {
                            b"terminate\0"
                        };
                        let _ = msg0(a, action);
                        return;
                    }
                }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_frame_returns_plausible_frame() {
        // 主屏：frame 应该以 (0,0) 为原点且尺寸为正
        let main = screen_frame(ScreenChoice::Main).expect("主屏 frame 不应为 None");
        assert!((main.0 - 0.0).abs() < 1.0 && main.1.abs() < 1.0, "主屏原点应为 (0,0)");
        assert!(main.2 > 500.0 && main.3 > 400.0, "主屏尺寸不合理: {main:?}");

        // 鼠标屏：与某块屏幕的 frame 一致
        let cur = screen_frame(ScreenChoice::Cursor).expect("鼠标屏 frame 不应为 None");
        assert!(cur.2 > 0.0 && cur.3 > 0.0);

        // Dock 屏 / 活动屏：Dock 与某个前台 app 总在运行，应能解析
        let dock = screen_frame(ScreenChoice::Dock).expect("Dock 屏 frame 不应为 None");
        assert!(dock.2 > 0.0);
        let act = screen_frame(ScreenChoice::Active).expect("活动屏 frame 不应为 None");
        assert!(act.2 > 0.0);
    }

    #[test]
    fn app_running_finds_finder() {
        assert!(app_running_at(std::path::Path::new(
            "/System/Library/CoreServices/Finder.app"
        )));
        assert!(!app_running_at(std::path::Path::new(
            "/Applications/DefinitelyNotRunningE2E.app"
        )));
    }

    #[test]
    fn pinch_monitor_installs_and_flags_roundtrip() {
        // 在测试进程里也应能注册成功（NSEvent 类方法，无需 NSApp）
        assert!(install_pinch_monitor(), "捏合监听注册失败");
        set_pinch_fingers(3);
        assert_eq!(take_pinch(), 0, "无手势时 take 应为 0");
        set_pinch_fingers(0);
    }

    #[test]
    fn trash_sound_does_not_panic() {
        play_trash_sound();
    }
}

/// 把一组路径移入废纸篓（NSWorkspace recycleURLs，无需 Finder 自动化权限）
pub fn trash_paths(paths: &[String]) -> bool {
    unsafe {
        let arr = msg0(cls(b"NSMutableArray\0"), b"array\0");
        if arr.is_null() {
            return false;
        }
        for p in paths {
            let url = msg1(cls(b"NSURL\0"), b"fileURLWithPath:\0", nsstring(p));
            if !url.is_null() {
                msg_void1(arr, b"addObject:\0", url);
            }
        }
        let ws = msg0(cls(b"NSWorkspace\0"), b"sharedWorkspace\0");
        if ws.is_null() {
            return false;
        }
        let f: extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) -> bool =
            std::mem::transmute(objc_msgSend as *const ());
        f(
            ws,
            sel(b"recycleURLs:completionHandler:\0"),
            arr,
            std::ptr::null_mut(),
        )
    }
}
