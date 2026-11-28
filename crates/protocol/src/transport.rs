//! 统一二维码传输参数、UR编码、喷泉码恢复和完整性检查；不改变业务或签名字节。

use crc::{Crc, CRC_32_ISO_HDLC};

pub const FRAGMENT_BYTES: usize = 400;
pub const INTERVAL_MS: u64 = 800;
pub const FOREGROUND: &str = "#007A74";
pub const BACKGROUND: &str = "#FFFFFF";
pub const QUIET_ZONE: usize = 4;
pub const MAX_VERSION: usize = 20;
pub const IDLE_MS: u64 = 300_000;
pub const MAX_FRAME_BYTES: usize = 2048;
// 宿主传输容纳塔塔既有大请求；各产品仍执行自身更严格的业务上限。
pub const MAX_MESSAGE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_PARTS: usize = (MAX_MESSAGE_BYTES + 9).div_ceil(FRAGMENT_BYTES);
pub fn profile() -> String {
    format!(r##"{{"fragment_bytes":{FRAGMENT_BYTES},"interval_ms":{INTERVAL_MS},"foreground":"{FOREGROUND}","background":"{BACKGROUND}","quiet_zone":{QUIET_ZONE},"max_version":{MAX_VERSION},"frame_bytes":{MAX_FRAME_BYTES},"message_bytes":{MAX_MESSAGE_BYTES},"idle_ms":{IDLE_MS}}}"##)
}

const UR_TYPE: &str = "citizen";
const CRC: Crc<u32> = Crc::<u32>::new(&CRC_32_ISO_HDLC);

// 平台只提供单调时钟；超时重置和恢复规则仍只在公共核心实现。
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" { fn qr_now_ms() -> f64; }
fn now_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    { unsafe { qr_now_ms().max(0.0) as u64 } }
    #[cfg(not(target_arch = "wasm32"))]
    {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
    }
}

fn cbor_message(text: &str) -> Result<Vec<u8>, String> {
    if text.is_empty() || text.len() > MAX_MESSAGE_BYTES {
        return Err("二维码消息长度超限".into());
    }
    let mut writer = minicbor::Encoder::new(Vec::new());
    writer.bytes(text.as_bytes()).map_err(|_| "二维码消息封装失败")?;
    Ok(writer.into_writer())
}

fn message_text(bytes: &[u8]) -> Result<String, String> {
    let mut reader = minicbor::Decoder::new(bytes);
    let text = reader.bytes().map_err(|_| "二维码消息不是字节串")?;
    if text.is_empty() || text.len() > MAX_MESSAGE_BYTES || reader.position() != bytes.len() {
        return Err("二维码消息封装无效".into());
    }
    String::from_utf8(text.to_vec()).map_err(|_| "二维码消息不是UTF-8".into())
}

/// 短消息只显示一张；长消息持续产生新的冗余帧，不循环固定片段。
pub struct Encoder {
    single: Option<String>,
    fountain: Option<ur::Encoder>,
    sequence: usize,
}

impl Encoder {
    pub fn new(text: &str) -> Result<Self, String> {
        let message = cbor_message(text)?;
        let frame = ur::encode(&message, UR_TYPE).to_ascii_uppercase();
        // Model 2版本20/M可容纳970个字母数字字符；静态判定与400字节分片独立。
        if frame.len() <= 970 {
            return Ok(Self { single: Some(frame), fountain: None, sequence: 0 });
        }
        let fountain = ur::Encoder::new(&message, FRAGMENT_BYTES, UR_TYPE)
            .map_err(|error| format!("二维码分片失败：{error}"))?;
        Ok(Self { single: None, fountain: Some(fountain), sequence: 0 })
    }

    pub fn animated(&self) -> bool { self.fountain.is_some() }

    pub fn next_frame(&mut self) -> Result<String, String> {
        if let Some(frame) = &self.single { return Ok(frame.clone()); }
        // 所选上游URI解析器使用u16序号；不能产生接收方无法解析的帧。
        if self.sequence >= u16::MAX as usize { return Err("二维码播放会话已结束，请重新打开".into()); }
        self.sequence += 1;
        let frame = self.fountain.as_mut().ok_or("二维码编码器已关闭")?
            .next_part().map_err(|error| format!("二维码编码失败：{error}"))?
            .to_ascii_uppercase();
        if frame.len() > MAX_FRAME_BYTES { return Err("二维码帧长度超限".into()); }
        Ok(frame)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Metadata { count: usize, length: usize, checksum: u32, fragment: usize }

/// 进度只是估计值；只有恢复、CRC及消息封装全部核验后才返回完整消息。
#[derive(Debug, PartialEq)]
pub struct Received { pub progress: u8, pub text: Option<String> }

#[derive(Default)]
pub struct Decoder {
    fountain: ur::Decoder,
    metadata: Option<Metadata>,
    received: usize,
    completed: Option<String>,
    updated: Option<u64>,
    seen: std::collections::BTreeMap<u32, u32>,
}

impl Decoder {
    pub fn receive(&mut self, raw: &str) -> Result<Received, String> {
        self.receive_at(raw, now_ms())
    }

    pub fn receive_at(&mut self, raw: &str, now: u64) -> Result<Received, String> {
        if self.updated.is_some_and(|updated| now.saturating_sub(updated) >= IDLE_MS) { *self = Self::default(); }
        if let Some(text) = &self.completed { return Ok(Received { progress: 100, text: Some(text.clone()) }); }
        if raw.is_empty() || raw.len() > MAX_FRAME_BYTES || !raw.is_ascii() {
            return Err("二维码帧长度或字符无效".into());
        }
        let frame = raw.to_ascii_lowercase();
        let body = frame.strip_prefix("ur:citizen/").ok_or("二维码传输类型不符")?;
        let (kind, bytes) = ur::decode(&frame).map_err(|error| format!("二维码帧校验失败：{error}"))?;
        if matches!(kind, ur::ur::Kind::SinglePart) {
            if self.metadata.is_some() { return Err("二维码不属于当前消息".into()); }
            let text = message_text(&bytes)?;
            self.updated = Some(now);
            self.completed = Some(text.clone());
            return Ok(Received { progress: 100, text: Some(text) });
        }
        let (indices, _) = body.split_once('/').ok_or("二维码帧缺少编号")?;
        let (sequence, count) = indices.split_once('-').ok_or("二维码帧编号无效")?;
        let sequence: u32 = sequence.parse().map_err(|_| "二维码帧编号无效")?;
        let count: usize = count.parse().map_err(|_| "二维码帧数量无效")?;
        let mut reader = minicbor::Decoder::new(&bytes);
        if reader.array().map_err(|_| "二维码帧结构无效")? != Some(5) { return Err("二维码帧结构无效".into()); }
        let part_sequence = reader.u32().map_err(|_| "二维码帧编号无效")?;
        let part_count = reader.u32().map_err(|_| "二维码帧数量无效")? as usize;
        let length = reader.u32().map_err(|_| "二维码消息长度无效")? as usize;
        let checksum = reader.u32().map_err(|_| "二维码校验值无效")?;
        let fragment = reader.bytes().map_err(|_| "二维码分片无效")?.len();
        if sequence == 0 || sequence != part_sequence || count != part_count || count == 0 || count > MAX_PARTS
            || length == 0 || length > MAX_MESSAGE_BYTES + 9 || fragment == 0 || fragment > FRAGMENT_BYTES
            || length.div_ceil(fragment) != count || reader.position() != bytes.len() {
            return Err("二维码分片边界无效".into());
        }
        let metadata = Metadata { count, length, checksum, fragment };
        if self.metadata.is_some_and(|previous| previous != metadata) { return Err("二维码不属于当前消息".into()); }
        let digest = CRC.checksum(&bytes);
        if let Some(previous) = self.seen.get(&sequence) {
            if *previous != digest { return Err("二维码同一帧内容冲突".into()); }
            self.updated = Some(now);
            return Ok(Received { progress: self.fountain.progress().min(99), text: None });
        }
        // 限制每个窗口的有效处理预算，重复或恶意输入不能无限积累状态。
        if self.received >= MAX_PARTS * 4 { return Err("二维码接收会话超限，请重新扫描".into()); }
        self.fountain.receive(&frame).map_err(|error| format!("二维码恢复失败：{error}"))?;
        self.seen.insert(sequence, digest);
        self.updated = Some(now);
        self.metadata = Some(metadata);
        self.received += 1;
        if self.fountain.complete() {
            let message = match self.fountain.message() {
                Ok(Some(message)) => message,
                result => { let error=format!("二维码恢复失败：{result:?}"); *self=Self::default(); return Err(error); }
            };
            if message.len() != length || CRC.checksum(&message) != checksum { *self=Self::default(); return Err("二维码完整性校验失败".into()); }
            let text = match message_text(&message) { Ok(text)=>text,Err(error)=>{*self=Self::default();return Err(error)} };
            self.completed = Some(text.clone());
            return Ok(Received { progress: 100, text: Some(text) });
        }
        Ok(Received { progress: self.fountain.progress().min(99), text: None })
    }
}
