use super::types::{SyncCollection, SyncIndex};
use super::webdav_client::WebdavClient;

pub async fn load_index(client: &WebdavClient, collection: SyncCollection) -> Result<SyncIndex, String> {
    let path = format!("{}/index.json", collection.dir());
    let index = client.get_json(&path).await?;
    if index.is_some() {
        client.mark_dir_ensured("");
        client.mark_dir_ensured(collection.dir());
        client.mark_dir_ensured(&format!("{}/chunks", collection.dir()));
    }
    Ok(index.unwrap_or_default())
}

// 合并写 index:写前先 load 远端当前索引,与本地内存索引在内存合并——
// 本地条目覆盖同名 uuid、远端独有条目保留、next_chunk 取两者较大。
// 返回合并结果与"是否相对远端有实质变化",调用方仅在 has_change 时
// 条件 PUT。这解决 index.json 裸 PUT 整块覆盖对端并发写入的问题
// (chunk 有的先 load 再合并,index 此前没有)。
pub async fn merge_index(
    client: &WebdavClient,
    collection: SyncCollection,
    local: &SyncIndex,
) -> Result<(SyncIndex, bool), String> {
    let remote = load_index(client, collection).await?;
    let mut merged = remote.clone();
    merged.next_chunk = merged.next_chunk.max(local.next_chunk);
    let mut changed = merged.next_chunk != remote.next_chunk;
    for (uuid, entry) in &local.entries {
        let is_new_or_newer = match remote.entries.get(uuid) {
            Some(existing) => {
                existing.chunk != entry.chunk
                    || existing.updated_at != entry.updated_at
                    || existing.source_device_id != entry.source_device_id
            }
            None => true,
        };
        if is_new_or_newer {
            merged.entries.insert(uuid.clone(), entry.clone());
            changed = true;
        }
    }
    Ok((merged, changed))
}

pub async fn save_index(
    client: &WebdavClient,
    collection: SyncCollection,
    index: &SyncIndex,
) -> Result<(), String> {
    let path = format!("{}/index.json", collection.dir());
    client.put_json(&path, index).await
}
