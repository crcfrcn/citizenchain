//! 为原生适配和浏览器公开同一有界传输会话；只处理公开消息，不接收密钥。

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use crate::transport::{Decoder, Encoder, MAX_FRAME_BYTES, MAX_MESSAGE_BYTES, profile};

enum Session { Encode(Encoder), Decode(Decoder) }
struct Entry { session: Session, pending: Option<Vec<u8>> }
#[derive(Default)]
struct Store { next: u32, entries: BTreeMap<u32, Entry> }
static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
fn store() -> &'static Mutex<Store> { STORE.get_or_init(|| Mutex::new(Store::default())) }

pub fn create(mode: u32, input: &[u8]) -> Result<u32, String> {
    let session = match mode {
        1 => Session::Encode(Encoder::new(std::str::from_utf8(input).map_err(|_| "二维码消息不是UTF-8")?)?),
        2 if input.is_empty() => Session::Decode(Decoder::default()),
        _ => return Err("二维码会话类型无效".into()),
    };
    let mut owner = store().lock().map_err(|_| "二维码会话不可用")?;
    if owner.entries.len() >= 128 { return Err("二维码会话数量超限".into()); }
    owner.next = owner.next.checked_add(1).ok_or("二维码会话编号耗尽")?;
    let handle = owner.next;
    owner.entries.insert(handle, Entry { session, pending: None });
    Ok(handle)
}

pub fn release(handle: u32) { if let Ok(mut owner) = store().lock() { owner.entries.remove(&handle); } }

/// 包头：0=接收中，1=显示帧，2=完整消息，3=配置，255=错误；第二字节为进度或动画标志。
pub fn step(handle: u32, input: &[u8], capacity: usize) -> Result<Vec<u8>, usize> {
    if handle == 0 {
        let mut value = vec![3, 0]; value.extend_from_slice(profile().as_bytes());
        return if capacity < value.len() { Err(value.len()) } else { Ok(value) };
    }
    let Ok(mut owner) = store().lock() else { return Ok(vec![255, 0]); };
    let Some(entry) = owner.entries.get_mut(&handle) else { return Ok(vec![255, 0]); };
    if entry.pending.is_none() {
        let result: Result<Vec<u8>, String> = (|| match &mut entry.session {
            Session::Encode(encoder) => {
                if !input.is_empty() { return Err("编码会话不接受扫描输入".into()); }
                let mut value = vec![1, u8::from(encoder.animated())];
                value.extend_from_slice(encoder.next_frame()?.as_bytes()); Ok(value)
            }
            Session::Decode(decoder) => {
                let raw = std::str::from_utf8(input).map_err(|_| "二维码帧不是UTF-8")?;
                let received = decoder.receive(raw)?;
                let mut value = vec![if received.text.is_some() { 2 } else { 0 }, received.progress];
                if let Some(text) = received.text { value.extend_from_slice(text.as_bytes()); }
                Ok(value)
            }
        })();
        entry.pending = Some(result.unwrap_or_else(|error| { let mut value = vec![255, 0]; value.extend_from_slice(error.as_bytes()); value }));
    }
    let value = entry.pending.as_ref().expect("刚准备的输出必须存在");
    if capacity < value.len() { return Err(value.len()); }
    Ok(entry.pending.take().expect("已核对的输出必须存在"))
}

/// C/WASM调用方负责有效内存及对应长度；失败不读取空指针或越界输入。
pub unsafe fn open(mode: u32, input: *const u8, length: usize) -> u32 {
    if length > MAX_MESSAGE_BYTES || input.is_null() && length != 0 { return 0; }
    let bytes = if length == 0 { &[] } else { unsafe { std::slice::from_raw_parts(input, length) } };
    create(mode, bytes).unwrap_or(0)
}

/// 缓冲区不足返回负的所需长度，缓存本次结果，重试不会推进或重复接收帧。
pub unsafe fn copy(handle: u32, input: *const u8, length: usize, output: *mut u8, capacity: usize) -> isize {
    if length > MAX_FRAME_BYTES || input.is_null() && length != 0 || output.is_null() && capacity != 0 { return -1; }
    let bytes = if length == 0 { &[] } else { unsafe { std::slice::from_raw_parts(input, length) } };
    match step(handle, bytes, capacity) {
        Err(required) => -(required as isize),
        Ok(value) => { if !value.is_empty() { unsafe { std::ptr::copy_nonoverlapping(value.as_ptr(), output, value.len()); } } value.len() as isize }
    }
}

// 消费者只导出自己的入口名，原生产品不会引入浏览器分配器或第二套算法。
#[macro_export]
macro_rules! export_transport {
    ($open:ident, $step:ident, $close:ident) => {
        #[no_mangle]
        pub unsafe extern "C" fn $open(mode: u32, input: *const u8, length: usize) -> u32 {
            std::panic::catch_unwind(|| unsafe { $crate::wasm::open(mode, input, length) }).unwrap_or(0)
        }
        #[no_mangle]
        pub unsafe extern "C" fn $step(handle: u32, input: *const u8, length: usize, output: *mut u8, capacity: usize) -> isize {
            std::panic::catch_unwind(|| unsafe { $crate::wasm::copy(handle, input, length, output, capacity) }).unwrap_or(-1)
        }
        #[no_mangle]
        pub extern "C" fn $close(handle: u32) { $crate::wasm::release(handle); }
    };
}

#[cfg(feature = "wasm")]
crate::export_transport!(qr_open, qr_step, qr_close);

#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn qr_alloc(length: usize) -> *mut u8 {
    if length == 0 || length > MAX_MESSAGE_BYTES + 2048 { return std::ptr::null_mut(); }
    Box::into_raw(vec![0u8; length].into_boxed_slice()).cast::<u8>()
}

#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
#[no_mangle]
pub unsafe extern "C" fn qr_free(pointer: *mut u8, length: usize) {
    if !pointer.is_null() && length > 0 && length <= MAX_MESSAGE_BYTES + 2048 {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(pointer, length)) });
    }
}
