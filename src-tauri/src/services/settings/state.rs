use super::{AppSettings, storage::SettingsStorage};
use once_cell::sync::Lazy;
use parking_lot::RwLock;

static SETTINGS: Lazy<RwLock<AppSettings>> = Lazy::new(|| {
    RwLock::new(SettingsStorage::load().unwrap_or_default())
});

pub fn get_settings() -> AppSettings {
    SETTINGS.read().clone()
}

// 热路径只读单字段:edge_monitor 50ms 轮询若走 get_settings() 会深克隆 ~135 字段,
// 这里只读锁 + bool 拷贝,20Hz × 2 路径降到常数级。
pub fn is_edge_hover_popup_enabled() -> bool {
    SETTINGS.read().edge_hover_popup_enabled
}

pub fn update_settings(mut settings: AppSettings) -> Result<(), String> {
    // 守不变量:所有写入路径统一入口,杜绝 hide=false/hover=true 违规组合落地
    settings.normalize_edge_hover_invariant();
    *SETTINGS.write() = settings.clone();
    SettingsStorage::save(&settings)
}

pub fn update_with<F>(updater: F) -> Result<(), String>
where
    F: FnOnce(&mut AppSettings),
{
    let mut settings = SETTINGS.write();
    updater(&mut settings);
    // 守不变量:update_with 是与 update_settings 并列的写入入口,
    // 必须共享归一化,杜绝 hide=false/hover=true 违规组合落地
    settings.normalize_edge_hover_invariant();
    // 克隆一份给 IO,并在 save 之前释放写锁——
    // 否则 SettingsStorage::save 的序列化+磁盘写会阻塞 50ms,
    // 期间所有 get_settings() 读路径(edge_monitor 50ms 轮询)被锁死
    let snapshot = settings.clone();
    drop(settings);
    SettingsStorage::save(&snapshot)
}

pub fn get_data_directory() -> Result<std::path::PathBuf, String> {
    let settings = SETTINGS.read();
    SettingsStorage::get_data_directory(&settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // 串行化所有写 SETTINGS 的测试。
    // 否则并发跑时 update_settings / update_with 互相覆盖,
    // 读到对方刚写入的 hide=true/hover=true,误判归一化失败。
    static SERIAL: Mutex<()> = Mutex::new(());

    fn lock_serial() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    // update_settings(所有写入统一入口)必须执行归一化
    #[test]
    fn update_settings_normalizes_edge_hover_invariant() {
        let _g = lock_serial();
        let mut settings = AppSettings::default();
        settings.edge_hide_enabled = false;
        settings.edge_hover_popup_enabled = true;
        update_settings(settings.clone()).unwrap();
        let loaded = get_settings();
        assert!(
            !loaded.edge_hover_popup_enabled,
            "update_settings 必须归一化,杜绝 hide=false/hover=true 落地"
        );
        // 恢复默认,避免污染其他测试
        update_settings(AppSettings::default()).unwrap();
    }

    // 热路径 accessor:只读 edge_hover_popup_enabled,与 get_settings 同源但零深克隆。
    // edge_monitor 50ms 轮询必须走此入口(源码护栏在 edge_monitor 测试模块)。
    #[test]
    fn is_edge_hover_popup_enabled_reads_live_field() {
        let _g = lock_serial();
        let mut settings = AppSettings::default();
        settings.edge_hide_enabled = true;
        settings.edge_hover_popup_enabled = true;
        update_settings(settings).unwrap();
        assert!(
            is_edge_hover_popup_enabled(),
            "写入 true 后 accessor 必须读到 true"
        );

        let mut settings = AppSettings::default();
        settings.edge_hide_enabled = true;
        settings.edge_hover_popup_enabled = false;
        update_settings(settings).unwrap();
        assert!(
            !is_edge_hover_popup_enabled(),
            "写入 false 后 accessor 必须读到 false"
        );

        update_settings(AppSettings::default()).unwrap();
    }

    // update_with 也必须与 update_settings 共享归一化
    #[test]
    fn update_with_normalizes_edge_hover_invariant() {
        let _g = lock_serial();
        let mut seed = AppSettings::default();
        seed.edge_hide_enabled = true;
        seed.edge_hover_popup_enabled = true;
        update_settings(seed).unwrap();

        update_with(|s| {
            s.edge_hide_enabled = false;
        })
        .expect("update_with 必须支持 hide 字段变更");

        let loaded = get_settings();
        assert!(
            !loaded.edge_hover_popup_enabled,
            "update_with 必须归一化 hide=false ⇒ hover=false,现状 hover={}",
            loaded.edge_hover_popup_enabled
        );
        assert!(
            !loaded.edge_hide_enabled,
            "update_with 应保留 hide=false 写入"
        );

        update_settings(AppSettings::default()).unwrap();
    }

    // A3 护栏:update_with 必须在 SettingsStorage::save 之前释放 RwLock 写锁,
    // 否则 save 的 IO 阻塞期间所有 get_settings() 阻塞 50ms,
    // edge_monitor 轮询等读路径被锁卡死。
    // ponytail: 全局 RwLock 不拆分(per-field lock 是过度工程,save 本就序列化写),
    // 但 save 期间必须显式释放写锁——这是单一调用点必守的最小契约。
    #[test]
    fn update_with_releases_lock_before_io_save() {
        let _g = lock_serial();
        let body = std::fs::read_to_string(format!(
            "{}/src/services/settings/state.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 state.rs 源文件");

        // 抽取 update_with 函数体范围
        let fn_start = body.find("pub fn update_with").expect("找不到 update_with");
        // 找下一个顶级定义边界("\npub fn " 或 "\nfn "),剥离闭包/impl 干扰
        let fn_body_end_rel = body[fn_start..]
            .find("\npub fn ")
            .or_else(|| body[fn_start..].find("\nfn "))
            .unwrap_or(body.len() - fn_start);
        let fn_body = &body[fn_start..fn_start + fn_body_end_rel];

        // 剥行注释后再匹配(§10.3:负向/顺序断言必须先剥注释,否则注释里的字面误命中)
        let bare: String = fn_body
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        // 1. 函数体内必须显式 drop 写锁 guard
        assert!(
            bare.contains("drop(settings)"),
            "update_with 必须在 save 之前 drop(settings) 释放 RwLockWriteGuard,\
             否则 save IO 阻塞期间所有 get_settings() 阻塞 50ms,edge_monitor 轮询被锁死"
        );

        // 2. drop(settings) 必须在 SettingsStorage::save 之前(顺序不变量)
        let drop_pos = bare
            .find("drop(settings)")
            .expect("必须先有 drop(settings),本断言不该单独失败");
        let save_pos = bare
            .find("SettingsStorage::save")
            .expect("必须先有 SettingsStorage::save,本断言不该单独失败");
        assert!(
            drop_pos < save_pos,
            "drop(settings) 必须在 SettingsStorage::save 之前;\
             drop_pos={} save_pos={}",
            drop_pos, save_pos
        );
    }
}
