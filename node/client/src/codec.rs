//! 按真实Runtime元数据定位存储，解码SCALE账户并安全截断UTF-8展示文本。

#![allow(clippy::result_large_err)]
use runtime;
use std::collections::BTreeMap;

use codec::{Decode, Encode};

use frame_metadata::RuntimeMetadata;
use frame_metadata::RuntimeMetadataPrefixed;
use frame_metadata::META_RESERVED;

/// 原生规则使用编译期类型的键，物理存储名由执行区块的真实元数据决定。
/// 按稳定 pallet 编号定位前缀；不修改键后缀、SCALE 值或链状态，也不按版本猜测名称。
#[derive(Clone)]
pub struct RuntimeStorage {
    prefixes: Vec<([u8; 16], [u8; 16])>,
    entity_layouts: [Option<bool>; 2],
    migration: runtime::MigrationState,
}

/// 原始记录及其已确定的物理布局；旧账户字段不代表人员资格。
pub struct StorageRecord {
    /// 节点从所查询物理键取得的原始SCALE值。
    pub bytes: Vec<u8>,
    /// 布局由实际元数据或已验证迁移来源确定，不能试解码猜测。
    pub legacy: bool,
}

impl RuntimeStorage {
    fn names(bytes: &[u8]) -> Result<BTreeMap<u8, String>, String> {
        let metadata = decode_runtime_metadata(bytes)?;
        let names: Vec<_> = match metadata {
            frame_metadata::RuntimeMetadata::V14(value) => value
                .pallets
                .into_iter()
                .map(|p| (p.index, p.name, p.storage.map(|storage| storage.prefix)))
                .collect(),
            frame_metadata::RuntimeMetadata::V15(value) => value
                .pallets
                .into_iter()
                .map(|p| (p.index, p.name, p.storage.map(|storage| storage.prefix)))
                .collect(),
            frame_metadata::RuntimeMetadata::V16(value) => value
                .pallets
                .into_iter()
                .map(|p| (p.index, p.name, p.storage.map(|storage| storage.prefix)))
                .collect(),
            _ => return Err("节点守卫不支持该 Runtime 元数据".into()),
        };
        let mut indexed = BTreeMap::new();
        let mut occupied = std::collections::BTreeSet::new();
        let mut storage_prefixes = std::collections::BTreeSet::new();
        for (index, name, storage) in names {
            let prefix = storage.unwrap_or_else(|| name.clone());
            if name.is_empty()
                || prefix.is_empty()
                || indexed.insert(index, prefix.clone()).is_some()
                || !occupied.insert(name)
                || !storage_prefixes.insert(prefix)
            {
                return Err("Runtime 元数据包含空名称或重复 pallet 身份".into());
            }
        }
        Ok(indexed)
    }

    /// 根据执行 Runtime 的完整元数据建立存储定位，重复身份和受保护模块缺失均失败。
    pub fn from_metadata(bytes: &[u8]) -> Result<Self, String> {
        let compiled =
            runtime::Runtime::metadata_at_version(16).ok_or("节点编译期缺少 V16 元数据")?;
        let actual = Self::names(bytes)?;
        let mut prefixes = Vec::new();
        for (index, name) in Self::names(compiled.as_slice())? {
            if let Some(owner) = actual.get(&index) {
                let source = sp_crypto_hashing::twox_128(name.as_bytes());
                let target = sp_crypto_hashing::twox_128(owner.as_bytes());
                if source != target {
                    prefixes.push((source, target));
                }
            } else if matches!(name.as_str(), "System" | "Balances" | "Genesis") {
                return Err(format!("Runtime 元数据缺少受保护 pallet 编号 {index}"));
            }
        }
        let mut entity_layouts = [None, None];
        macro_rules! layouts {
            ($metadata:expr) => {
                for pallet in &$metadata.pallets {
                    if !matches!(pallet.index, 30 | 31) { continue; }
                    let storage = pallet.storage.as_ref().ok_or("实体元数据缺少存储声明")?;
                    let entry = storage.entries.iter().find(|s| s.name == "Institutions")
                        .ok_or("实体元数据缺少机构表")?;
                    let frame_metadata::v14::StorageEntryType::Map { value, .. } = &entry.ty
                    else { return Err("机构表不是映射".into()); };
                    entity_layouts[(pallet.index - 30) as usize] = Some(
                        runtime::guard_institution_layout(&$metadata.types, value.id)
                            .map_err(|e| format!("机构元数据布局非法:{e:?}"))?);
                }
            };
        }
        match decode_runtime_metadata(bytes)? {
            RuntimeMetadata::V14(value) => layouts!(value),
            RuntimeMetadata::V15(value) => layouts!(value),
            RuntimeMetadata::V16(value) => layouts!(value),
            _ => return Err("机构元数据版本不受支持".into()),
        }
        Ok(Self {
            prefixes,
            entity_layouts,
            migration: runtime::MigrationState::NotStarted,
        })
    }

    /// 机构原始值布局来自同一执行元数据；缺失声明不能视为新布局。
    pub fn entity_legacy(&self, public: bool) -> Result<bool, String> {
        self.entity_layouts[usize::from(!public)].ok_or("实体元数据缺失".into())
    }

    /// 迁移进度只经Runtime所属接口解析，错误记录不能退化成未迁移状态。
    pub fn with_state(
        mut self,
        read: impl Fn(&[u8]) -> Result<Option<Vec<u8>>, String>,
    ) -> Result<Self, String> {
        let error = std::cell::RefCell::new(None);
        let stage = runtime::guard_migration_state(|key| match read(key) {
            Ok(value) => value,
            Err(reason) => {
                *error.borrow_mut() = Some(reason);
                None
            }
        });
        if let Some(reason) = error.into_inner() {
            return Err(reason);
        }
        self.migration = stage.map_err(|e| format!("迁移阶段非法:{e:?}"))?;
        if self.migration != runtime::MigrationState::NotStarted
            && (self.entity_legacy(true)? || self.entity_legacy(false)?)
        {
            return Err("迁移标记与执行元数据布局冲突".into());
        }
        Ok(self)
    }

    /// 当前存储所属迁移阶段，仅供节点检查中间状态，不授予任何业务权限。
    pub fn migration(&self) -> runtime::MigrationState {
        self.migration
    }

    fn entity_key(key: &[u8]) -> Option<bool> {
        for public in [true, false] {
            let (_, target) = runtime::guard_entity_names(public);
            if key.starts_with(&sp_crypto_hashing::twox_128(target)) {
                return Some(public);
            }
        }
        None
    }

    /// 固定阶段内准确定位记录；迁移中才读取该迁移公开声明的未搬来源。
    pub fn read_raw(
        &self,
        key: &[u8],
        read: impl Fn(&[u8]) -> Result<Option<Vec<u8>>, String>,
    ) -> Result<Option<StorageRecord>, String> {
        let actual = self.key(key, true);
        let primary = read(&actual)?;
        let Some(public) = Self::entity_key(key) else {
            return Ok(primary.map(|bytes| StorageRecord {
                bytes,
                legacy: false,
            }));
        };
        if self.migration != runtime::MigrationState::Running {
            let legacy = self.entity_legacy(public)?;
            return Ok(primary.map(|bytes| StorageRecord { bytes, legacy }));
        }
        let (source, _) = runtime::guard_entity_names(public);
        let old_key = [sp_crypto_hashing::twox_128(source).as_slice(), &key[16..]].concat();
        let old = read(&old_key)?;
        match (primary, old) {
            (Some(bytes), Some(old)) => {
                if key.get(16..32)
                    == Some(sp_crypto_hashing::twox_128(b":__STORAGE_VERSION__:").as_slice())
                {
                    runtime::guard_entity_versions(public, &old, &bytes)
                        .map_err(|e| format!("FRAME初始化实体版本键非法:{e:?}"))?;
                    return Ok(Some(StorageRecord {
                        bytes,
                        legacy: false,
                    }));
                }
                if key.get(16..32)
                    != Some(sp_crypto_hashing::twox_128(b"InstitutionRoleAssignments").as_slice())
                {
                    return Err("迁移来源与目标记录冲突".into());
                }
                runtime::guard_partial_assignments(key, &old, &bytes, |key| {
                    read(key).map_err(|_| sp_runtime::DispatchError::Other("读取任职迁移状态失败"))
                })
                .map_err(|e| format!("任职迁移中间状态非法:{e:?}"))?;
                Ok(Some(StorageRecord {
                    bytes: old,
                    legacy: true,
                }))
            }
            (Some(bytes), _) => Ok(Some(StorageRecord {
                bytes,
                legacy: false,
            })),
            (None, Some(bytes)) => Ok(Some(StorageRecord {
                bytes,
                legacy: true,
            })),
            (None, None) => Ok(None),
        }
    }

    /// 永久事实统一由所属Runtime解释机构布局；其它值原样交给各自规则。
    pub fn read(
        &self,
        key: &[u8],
        read: impl Fn(&[u8]) -> Result<Option<Vec<u8>>, String>,
    ) -> Result<Option<Vec<u8>>, String> {
        let Some(record) = self.read_raw(key, read)? else {
            return Ok(None);
        };
        if Self::entity_key(key).is_some()
            && key.get(16..32) == Some(sp_crypto_hashing::twox_128(b"Institutions").as_slice())
        {
            return runtime::guard_institution(&record.bytes, record.legacy)
                .map(|info| Some(info.encode()))
                .map_err(|e| format!("机构事实解码失败:{e:?}"));
        }
        Ok(Some(record.bytes))
    }

    /// 迁移中的来源键仍由Runtime声明解释，未知来源不能当作普通无关键。
    pub fn logical_key(&self, key: &[u8]) -> Result<Vec<u8>, String> {
        if self.migration == runtime::MigrationState::Complete && runtime::guard_retired_key(key) {
            return Err("迁移完成后仍存在已退役来源键".into());
        }
        if self.migration == runtime::MigrationState::Running {
            if let Some(target) =
                runtime::guard_migration_target(key).map_err(|e| format!("迁移键非法:{e:?}"))?
            {
                return Ok(target);
            }
        }
        Ok(self.key(key, false))
    }

    /// 枚举必须覆盖当前阶段的全部真实来源，不能漏掉尚未搬迁的机构记录。
    pub fn physical_prefixes(&self, logical: &[u8]) -> Vec<Vec<u8>> {
        let mut prefixes = vec![self.key(logical, true)];
        if self.migration == runtime::MigrationState::Running {
            if let Some(public) = Self::entity_key(logical) {
                let (source, _) = runtime::guard_entity_names(public);
                let prefix = [
                    sp_crypto_hashing::twox_128(source).as_slice(),
                    &logical[16..],
                ]
                .concat();
                if !prefixes.contains(&prefix) {
                    prefixes.push(prefix);
                }
            }
        }
        prefixes
    }

    /// 解释同一已执行区块的物理搬迁，只有已声明且真实存在的来源删除才能合并。
    /// 搬迁后立即消费仍保留逻辑删除，普通同值写入仍保留；不修改原始delta或提交状态。
    pub fn transition_changes(
        &self,
        post: &Self,
        raw: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
        read_parent: impl Fn(&[u8]) -> Result<Option<Vec<u8>>, String>,
    ) -> Result<BTreeMap<Vec<u8>, Option<Vec<u8>>>, String> {
        let read_post = |key: &[u8]| match raw.get(key) {
            Some(value) => Ok(value.clone()),
            None => read_parent(key),
        };
        let mut grouped = BTreeMap::<Vec<u8>, Vec<(&Vec<u8>, &Option<Vec<u8>>)>>::new();
        for (key, value) in raw {
            let parent_key = self.logical_key(key)?;
            // 完成块必须删除最后的来源键；完成态禁止的是残留/重写，不能拒绝本次真实删除。
            let post_key = if value.is_none()
                && self.migration == runtime::MigrationState::Running
                && post.migration == runtime::MigrationState::Complete
                && runtime::guard_retired_key(key)
            {
                parent_key.clone()
            } else {
                post.logical_key(key)?
            };
            let logical = if parent_key != *key {
                parent_key
            } else {
                post_key
            };
            grouped.entry(logical).or_default().push((key, value));
        }
        let mut delta = BTreeMap::new();
        for (logical, records) in grouped {
            let target = post.key(&logical, true);
            let mut source = None;
            for (key, value) in &records {
                // 删除记录的排序不代表来源身份；目标也可能在同一初始化中被消费。
                if value.is_none()
                    && **key != target
                    && runtime::guard_migration_target(key)
                        .map_err(|e| format!("迁移键非法:{e:?}"))?
                        .is_some_and(|target| target == logical)
                    && source.replace(*key).is_some()
                {
                    return Err("同一逻辑键存在多个声明迁移来源".into());
                }
            }
            let moved = if let Some(source) = source {
                (self.migration == runtime::MigrationState::Running
                    || post.migration == runtime::MigrationState::Running
                    || self.key(&logical, true) != target)
                    && read_parent(source)?.is_some()
                    && read_post(source)?.is_none()
            } else {
                false
            };
            if records.len() > 1
                && (!moved
                    || records.len() != 2
                    || !records.iter().all(|(key, _)| Some(*key) == source || **key == target)
                    // 双删除只允许消费刚搬入的目标，不能掩盖父状态已有的双来源冲突。
                    || raw.get(&target).is_some_and(Option::is_none)
                        && read_parent(&target)?.is_some())
            {
                return Err(format!(
                    "节点守卫存储键归属冲突：逻辑前缀{}，来源{}个，已验证搬迁{}",
                    hex::encode(&logical[..logical.len().min(32)]),
                    records.len(),
                    moved
                ));
            }
            let before = self.read(&logical, &read_parent)?;
            let after = post.read(&logical, &read_post)?;
            if moved && before == after && after.is_some() {
                continue;
            }
            for (key, value) in &records {
                if value.is_some()
                    && **key != post.key(&logical, true)
                    && !(post.migration == runtime::MigrationState::Running
                        && runtime::guard_migration_target(key)
                            .map_err(|e| format!("迁移键非法:{e:?}"))?
                            .is_some_and(|target| target == logical))
                {
                    return Err("变更写入了非所属存储命名空间".into());
                }
            }
            delta.insert(logical, after);
        }
        Ok(delta)
    }

    /// 父状态归写入它的 Runtime 所有；set_code更新代码后的首块尚未执行名称迁移。
    pub fn at_state(
        client: &crate::types::FullClient,
        at: <runtime::opaque::Block as sp_runtime::traits::Block>::Hash,
    ) -> Result<Self, String> {
        use sc_client_api::HeaderBackend as _;
        use sp_api::{Metadata as _, ProvideRuntimeApi as _};
        use sp_runtime::traits::Header as _;
        let header = client
            .header(at)
            .map_err(|e| e.to_string())?
            .ok_or("存储所属区块不存在")?;
        let writer = if *header.number() == 0 {
            at
        } else {
            *header.parent_hash()
        };
        let metadata = client
            .runtime_api()
            .metadata(writer)
            .map_err(|e| e.to_string())?;
        Self::from_metadata(metadata.as_slice())
    }

    /// actual为true时定位真实读取键，为false时转为原生规则的键；后缀原样保留。
    pub fn key(&self, key: &[u8], actual: bool) -> Vec<u8> {
        for (compiled, runtime) in &self.prefixes {
            let (from, to) = if actual {
                (compiled, runtime)
            } else {
                (runtime, compiled)
            };
            if key.starts_with(from) {
                return [to.as_slice(), &key[16..]].concat();
            }
        }
        key.to_vec()
    }

    /// 为原生规则组织只读变更视图，保留删除事实并拒绝归属冲突。
    pub fn changes(
        &self,
        raw: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    ) -> Result<BTreeMap<Vec<u8>, Option<Vec<u8>>>, String> {
        let mut result = BTreeMap::new();
        for (key, value) in raw {
            if result.insert(self.key(key, false), value.clone()).is_some() {
                return Err("节点守卫存储键归属冲突".into());
            }
        }
        Ok(result)
    }
}

/// 父块事件解码和RPC能力探测只信任该块Runtime自身的完整元数据。
pub fn decode_runtime_metadata(raw: &[u8]) -> Result<RuntimeMetadata, String> {
    let mut input = raw;
    let RuntimeMetadataPrefixed(prefix, metadata) = RuntimeMetadataPrefixed::decode(&mut input)
        .map_err(|_| "Runtime元数据SCALE解码失败".to_string())?;
    if prefix != META_RESERVED || !input.is_empty() {
        return Err("Runtime元数据前缀或尾随字节非法".into());
    }
    if !matches!(
        &metadata,
        RuntimeMetadata::V14(_) | RuntimeMetadata::V15(_) | RuntimeMetadata::V16(_)
    ) {
        return Err("Runtime元数据版本不受支持".into());
    }
    Ok(metadata)
}

// ──── SCALE 解码 ────

/// 使用本链真实 AccountInfo 类型，拒绝截断及尾随数据，不以固定偏移猜测余额。
pub type AccountInfo = frame_system::AccountInfo<
    runtime::Nonce,
    <runtime::Runtime as frame_system::Config>::AccountData,
>;

pub fn decode_account(data: &[u8]) -> Result<AccountInfo, String> {
    use codec::DecodeAll;
    AccountInfo::decode_all(&mut &data[..]).map_err(|e| format!("System::Account 解码失败: {e}"))
}

/// 总余额包含保留资金及账户保留金；冻结额限制支出，不重复计入资产。
pub fn account_total(account: &AccountInfo) -> Result<u128, String> {
    account
        .data
        .free
        .checked_add(account.data.reserved)
        .ok_or_else(|| "System::Account 总余额溢出".to_string())
}

pub fn decode_hex_storage(hex_str: &str) -> Result<Vec<u8>, String> {
    let clean = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    hex::decode(clean).map_err(|e| format!("hex 解码失败: {e}"))
}

/// 严格解码提案 ID 列表；拒绝旧宽度、截断或尾随字节。
pub fn decode_u128_vec(data: &[u8]) -> Result<Vec<u128>, String> {
    use codec::DecodeAll;
    Vec::<u128>::decode_all(&mut &data[..]).map_err(|e| format!("提案 ID 列表解码失败: {e}"))
}

pub fn read_compact_u32(data: &[u8], offset: usize) -> Result<(u32, usize), String> {
    if offset >= data.len() {
        return Err("Compact<u32> 数据不足".to_string());
    }
    let first = data[offset];
    let mode = first & 0x03;
    match mode {
        0 => Ok(((first >> 2) as u32, 1)),
        1 => {
            if offset + 2 > data.len() {
                return Err("Compact<u32> two-byte 数据不足".to_string());
            }
            let value = (((data[offset + 1] as u32) << 8) | first as u32) >> 2;
            Ok((value, 2))
        }
        2 => {
            if offset + 4 > data.len() {
                return Err("Compact<u32> four-byte 数据不足".to_string());
            }
            let value = ((data[offset + 3] as u32) << 24)
                | ((data[offset + 2] as u32) << 16)
                | ((data[offset + 1] as u32) << 8)
                | (data[offset] as u32);
            Ok((value >> 2, 4))
        }
        _ => Err("Compact<u32> big-integer 模式暂不支持".to_string()),
    }
}

/// 从当前位置解码 SCALE `CidNumber`，并推进 offset。
pub fn decode_cid_number(data: &[u8], offset: &mut usize) -> Option<String> {
    let (len, len_len) = read_compact_u32(data, *offset).ok()?;
    *offset = offset.checked_add(len_len)?;
    let end = offset.checked_add(len as usize)?;
    if end > data.len() {
        return None;
    }
    let cid = core::str::from_utf8(&data[*offset..end]).ok()?.to_string();
    *offset = end;
    Some(cid)
}

/// 从当前位置解码 SCALE `RoleCode`，并推进 offset。
pub fn decode_role_code(data: &[u8], offset: &mut usize) -> Option<String> {
    let (len, len_len) = read_compact_u32(data, *offset).ok()?;
    *offset = offset.checked_add(len_len)?;
    let end = offset.checked_add(len as usize)?;
    if end > data.len() {
        return None;
    }
    let role_code = core::str::from_utf8(&data[*offset..end]).ok()?.to_string();
    *offset = end;
    Some(role_code)
}

pub fn decode_subject_cid_numbers(data: &[u8], offset: usize) -> Option<(Vec<String>, usize)> {
    let (count, count_len) = read_compact_u32(data, offset).ok()?;
    let mut pos = offset + count_len;
    let mut cids = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (len, len_len) = read_compact_u32(data, pos).ok()?;
        pos += len_len;
        let end = pos.checked_add(len as usize)?;
        if end > data.len() {
            return None;
        }
        cids.push(String::from_utf8_lossy(&data[pos..end]).to_string());
        pos = end;
    }
    Some((cids, pos))
}

// ──── summary 格式化(纯函数,便于单测) ────

/// 安全截断 UTF-8 字符串(按 Unicode 字符数计,避免切中 multi-byte 字符)。
pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

#[cfg(test)]
#[path = "../tests/codec.rs"]
mod tests;

/// 提案 ID 和年内序号在 JSON 边界使用十进制字符串，避免 JavaScript 丢失精度。
pub fn serialize_decimal<S: serde::Serializer, T: core::fmt::Display>(
    value: &T,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

/// 查询结果中的钱包资金摘要；不参与节点守卫和交易放行。
#[derive(Debug)]
pub struct TransactionActivity {
    pub extrinsic_index: u32,
    pub account_ids: Vec<[u8;32]>,
    pub direction: &'static str,
    pub mode: String,
    pub transfer_count: u32,
    pub amount_fen: u128,
    pub fee_fen: u128,
    pub state: &'static str,
}

/// 按执行父元数据读取交易及实际事件，仅生成查询展示资料。
pub fn transaction_activity(body:&[sp_runtime::OpaqueExtrinsic],raw:&[u8],metadata:&[u8],accounts:&[[u8;32]])->Result<Vec<TransactionActivity>,String>{
    use scale_value::{Value,ValueDef,Composite};
    use std::collections::BTreeSet;
    fn field<'a>(v:&'a Value<()>,name:&str)->Option<&'a Value<()>>{
        match &v.value{ValueDef::Composite(Composite::Named(fields))|ValueDef::Variant(scale_value::Variant{values:Composite::Named(fields),..})=>fields.iter().find(|(n,_)|n==name).map(|(_,v)|v),_=>None}
    }
    fn parts(v:&Value<()>)->Result<(&str,Vec<&Value<()>>),String>{
        match &v.value{ValueDef::Variant(v)=>Ok((&v.name,v.values.values().collect())),_=>Err("查询变体类型无效".into())}
    }
    fn bytes(v:&Value<()>)->Option<Vec<u8>>{
        match &v.value{
            ValueDef::Composite(Composite::Unnamed(values))=>{
                if values.len()==1&&values[0].as_u128().is_none(){return bytes(&values[0]);}
                values.iter().map(|v|v.as_u128().and_then(|n|u8::try_from(n).ok())).collect()
            },_=>None
        }
    }
    fn account(v:&Value<()>)->Option<[u8;32]>{bytes(v)?.try_into().ok()}
    fn proof(v:&Value<()>)->Option<[u8;32]>{
        if let Some(value)=field(v,"account"){return account(value);}
        let fields=match &v.value{ValueDef::Composite(f)=>Some(f),ValueDef::Variant(v)=>Some(&v.values),_=>None}?;
        fields.values().find_map(proof)
    }
    fn calls(v:&Value<()>,items:&mut Vec<([u8;32],u128)>)->Result<String,String>{
        let (pallet,p)=parts(v)?;let call=p.first().ok_or("查询调用包装缺失")?;
        let (name,_)=parts(call)?;
        if matches!(pallet,"Onchain"|"OnchainTransaction")&&name=="transfer_with_remark"{
            items.push((field(call,"beneficiary_account_id").and_then(account).ok_or("查询收款账户缺失")?,field(call,"amount").and_then(Value::as_u128).ok_or("查询金额缺失")?));
            return Ok("transfer".into());
        }
        if pallet=="Utility"&&matches!(name,"batch"|"batch_all"|"force_batch"){
            let values=field(call,"calls").ok_or("查询批次清单缺失")?;
            if let ValueDef::Composite(values)=&values.value{for value in values.values(){calls(value,items)?;}}else{return Err("查询批次清单无效".into());}
            return Ok(name.into());
        }
        Ok("other".into())
    }
    let accounts=accounts.iter().copied().collect::<BTreeSet<_>>();if accounts.is_empty(){return Ok(vec![]);}
    macro_rules! events {($m:expr)=>{{$m.pallets.iter().find(|p|p.name=="System").and_then(|p|p.storage.as_ref()).and_then(|s|s.entries.iter().find(|e|e.name=="Events")).and_then(|e|match e.ty{frame_metadata::v14::StorageEntryType::Plain(t)=>Some(t.id),_=>None}).ok_or("查询事件元数据缺失")?}};}
    let (types,address,signature,call,extensions,event_ty)=match decode_runtime_metadata(metadata)?{
        RuntimeMetadata::V14(m)=>{
            let e=events!(m);let xt=m.types.resolve(m.extrinsic.ty.id).ok_or("查询交易元数据缺失")?;
            let ty=|name:&str|xt.type_params.iter().find(|p|p.name==name).and_then(|p|p.ty).map(|t|t.id).ok_or("查询交易参数缺失");
            let a=ty("Address")?;let s=ty("Signature")?;let c=ty("Call")?;
            let x=BTreeMap::from([(0,m.extrinsic.signed_extensions.into_iter().map(|e|(e.identifier,e.ty.id)).collect::<Vec<_>>())]);(m.types,a,s,c,x,e)
        },
        RuntimeMetadata::V15(m)=>{let e=events!(m);let x=BTreeMap::from([(0,m.extrinsic.signed_extensions.into_iter().map(|e|(e.identifier,e.ty.id)).collect::<Vec<_>>())]);(m.types,m.extrinsic.address_ty.id,m.extrinsic.signature_ty.id,m.extrinsic.call_ty.id,x,e)},
        RuntimeMetadata::V16(m)=>{let e=events!(m);let mut x=BTreeMap::new();for (version,indexes) in m.extrinsic.transaction_extensions_by_version{let mut fields=vec![];for index in indexes{let ext=m.extrinsic.transaction_extensions.get(index.0 as usize).ok_or("查询扩展索引无效")?;fields.push((ext.identifier.clone(),ext.ty.id));}x.insert(version,fields);}(m.types,m.extrinsic.address_ty.id,m.extrinsic.signature_ty.id,m.extrinsic.call_ty.id,x,e)},
        _=>return Err("查询元数据版本不支持".into()),
    };
    let decode=|input:&mut &[u8],ty:u32|scale_value::scale::decode_as_type(input,ty,&types).map(Value::remove_context).map_err(|error|format!("查询SCALE解码失败：{error}"));
    let mut input=raw;let events=decode(&mut input,event_ty)?;if !input.is_empty(){return Err("查询事件有尾随字节".into());}
    let ValueDef::Composite(event_values)=&events.value else{return Err("查询事件列表无效".into());};
    let mut output=vec![];
    for (index,opaque) in body.iter().enumerate(){
        let index=u32::try_from(index).map_err(|_|"查询索引溢出")?;
        let (&tag,mut input)=opaque.inner().split_first().ok_or("查询交易为空")?;let mut payer=None;
        let version=match tag{0x84=>{let v=decode(&mut input,address)?;let (_,f)=parts(&v)?;payer=f.first().and_then(|v|account(v));decode(&mut input,signature)?;Some(0)},0x45=>{let (&v,tail)=input.split_first().ok_or("查询扩展版本缺失")?;input=tail;Some(v)},0x04|0x05=>None,_=>return Err("查询交易格式不支持".into())};
        if let Some(version)=version{for (name,ty) in extensions.get(&version).ok_or("查询扩展元数据缺失")?{let value=decode(&mut input,*ty)?;if name==<chain_signing::mldsa::Authenticate<runtime::Runtime> as sp_runtime::traits::TransactionExtension<runtime::RuntimeCall>>::IDENTIFIER{payer=proof(&value);}}}
        let call=decode(&mut input,call)?;if !input.is_empty(){return Err("查询交易有尾随字节".into());}
        let mut items=vec![];let mode=calls(&call,&mut items)?;
        let mut paid=vec![];let mut actual=vec![];let mut movements=vec![];let mut item_states=vec![];let mut success=None;let mut interrupted=None;
        for record in event_values.values(){
            let Some(phase)=field(record,"phase") else{return Err("查询事件阶段缺失".into());};let (name,fields)=parts(phase)?;
            if name!="ApplyExtrinsic"||fields.first().and_then(|v|v.as_u128())!=Some(index as u128){continue;}
            let (pallet,outer)=parts(field(record,"event").ok_or("查询事件缺失")?)?;
            let event=outer.first().ok_or("查询事件包装缺失")?;let (name,_)=parts(event)?;
            match (pallet,name){
                ("System","ExtrinsicSuccess")=>success=Some(true),("System","ExtrinsicFailed")=>success=Some(false),
                ("Onchain"|"OnchainTransaction","FeePaid")=>paid.push((field(event,"account_id").and_then(account).ok_or("查询费用账户缺失")?,field(event,"fee").and_then(Value::as_u128).ok_or("查询费用缺失")?)),
                ("Onchain"|"OnchainTransaction","TransferWithRemark")=>actual.push((field(event,"from_account_id").and_then(account).ok_or("查询付款账户缺失")?,field(event,"beneficiary_account_id").and_then(account).ok_or("查询收款账户缺失")?,field(event,"amount").and_then(Value::as_u128).ok_or("查询本金缺失")?)),
                ("Balances","Transfer")=>movements.push((field(event,"from").and_then(account).ok_or("查询付款账户缺失")?,field(event,"to").and_then(account).ok_or("查询收款账户缺失")?,field(event,"amount").and_then(Value::as_u128).ok_or("查询本金缺失")?)),
                ("Utility","ItemCompleted")=>item_states.push(true),("Utility","ItemFailed")=>item_states.push(false),
                ("Utility","BatchInterrupted")=>interrupted=field(event,"index").and_then(Value::as_u128),_=>{},
            }
        }
        if actual.is_empty(){actual=movements;}
        let sender=payer.is_some_and(|p|accounts.contains(&p))&&!items.is_empty();
        let mut related=BTreeSet::new();let mut amount=0u128;let mut count=0u32;let mut internal=true;let mut movement_sender=false;
        if sender&&!items.is_empty(){related.insert(payer.unwrap());for (to,value) in &items{amount=amount.checked_add(*value).ok_or("查询金额溢出")?;count=count.checked_add(1).ok_or("查询笔数溢出")?;internal&=accounts.contains(to);if accounts.contains(to){related.insert(*to);}}}
        else{for (from,to,value) in &actual{if accounts.contains(from)||accounts.contains(to){movement_sender|=accounts.contains(from);internal&=accounts.contains(from)&&accounts.contains(to);amount=amount.checked_add(*value).ok_or("查询金额溢出")?;count=count.checked_add(1).ok_or("查询笔数溢出")?;if accounts.contains(from){related.insert(*from);}if accounts.contains(to){related.insert(*to);}}}}
        let fee=paid.iter().filter(|(account,_)|accounts.contains(account)).try_fold(0u128,|sum,(account,fee)|{related.insert(*account);sum.checked_add(*fee).ok_or("查询费用溢出")})?;
        if related.is_empty(){continue;}
        let succeeded=success.ok_or("查询交易结果缺失")?;
        let state=if !sender&&!movement_sender&&count>0{"success"}else if !succeeded{if mode=="batch_all"{"rolled_back"}else{"failed"}}else if item_states.iter().any(|s|!*s)||interrupted.is_some(){if item_states.iter().any(|s|*s){"partial"}else{"failed"}}else{"success"};
        output.push(TransactionActivity{extrinsic_index:index,account_ids:related.into_iter().collect(),direction:if sender||movement_sender{if internal{"internal"}else{"sent"}}else if count>0{"received"}else{"operation"},mode,transfer_count:count,amount_fen:amount,fee_fen:fee,state});
    }
    Ok(output)
}
