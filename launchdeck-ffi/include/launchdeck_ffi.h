#ifndef LAUNCHDECK_FFI_H
#define LAUNCHDECK_FFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ---- 数据结构 ---- */

typedef struct CAppInfo {
    const char *id;
    const char *name;
    const char *path;
    const char *version;
    const char *bundle_id;
    uint64_t size;
    unsigned char is_hidden;
    unsigned char is_system;
} CAppInfo;

typedef struct CAppSettings {
    int display_mode;          /* 0 = 窗口, 1 = 全屏 */
    unsigned char show_dock_icon;
    unsigned char show_menu_bar_icon;
    unsigned char launch_at_login;
    unsigned char auto_update;
    int theme;                 /* 0 = 跟随系统, 1 = 浅色, 2 = 深色 */
    int grid_size;             /* 0 = 小, 1 = 中, 2 = 大 */
    unsigned char animation_enabled;
} CAppSettings;

/* ---- 初始化 ---- */

int launchdeck_init(void);

/* ---- 应用扫描 ---- */

int launchdeck_scan_apps(CAppInfo **out_apps, size_t *out_count);
void launchdeck_free_apps(CAppInfo *apps, size_t count);

/* ---- 设置 ---- */

int launchdeck_get_settings(CAppSettings *out_settings);
int launchdeck_update_settings(const CAppSettings *settings);

/* ---- 内存管理 ---- */

void launchdeck_free_string(char *ptr);

#ifdef __cplusplus
}
#endif

#endif /* LAUNCHDECK_FFI_H */
