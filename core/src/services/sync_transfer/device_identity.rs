use once_cell::sync::Lazy;
use uuid::Uuid;

const DEVICE_ID_KEY: &str = "sync_transfer_device_id";
const LEGACY_SYNC_TRANSFER_LAN_DEVICE_ID_KEY: &str = "sync_transfer_lan_device_id";

static DEVICE_ID: Lazy<String> = Lazy::new(load_or_create_device_id);

pub fn device_id() -> String {
    DEVICE_ID.clone()
}

fn load_or_create_device_id() -> String {
    if let Some(id) = stored_device_id(DEVICE_ID_KEY) {
        return id;
    }

    if let Some(id) = stored_device_id(LEGACY_SYNC_TRANSFER_LAN_DEVICE_ID_KEY) {
        let _ = crate::services::store::set(DEVICE_ID_KEY, &id);
        return id;
    }

    let id = Uuid::new_v4().to_string();
    let _ = crate::services::store::set(DEVICE_ID_KEY, &id);
    id
}

fn stored_device_id(key: &str) -> Option<String> {
    crate::services::store::get::<String>(key)
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    // device_id 是进程级全局 Lazy,测试须串行化避免并发互踩。
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn lock_serial() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    // 首次调用返回 UUID 格式(v4 含 4 个连字符),进程内多次调用返回同一值。
    #[test]
    fn device_id_is_stable_uuid_within_process() {
        let _g = lock_serial();
        let first = device_id();
        let second = device_id();
        assert_eq!(first, second, "进程内 device_id 必须稳定");
        assert_eq!(first.len(), 36, "UUID v4 字符串长度应为 36");
        assert_eq!(first.chars().filter(|c| *c == '-').count(), 4, "UUID 应含 4 个连字符");
        assert!(first.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'), "UUID 应仅含字母数字与连字符");
    }
}
