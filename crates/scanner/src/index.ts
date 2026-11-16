/// GMB React 产品统一二维码设备适配器。
///
/// 设备识别输出原始字符串；传输帧助手只缓存帧，校验交给调用方，不判断业务码型或执行导航。
export {
  ScannerController,
  ScannerFailure,
  type QrDecoder,
  type ScannerCallbacks,
  type ScannerControllerDependencies,
  type ScannerControllerLike,
  type ScannerFailureKind,
} from './scanner';
export { ScannerView, type ScannerViewProps } from './ScannerView';

// 此处仅缓存传输帧，帧校验、哈希核对与解码复用统一 Rust 核心。
export function createQrAssembler(join: (frames: string[]) => Promise<string | null>) {
  let frames = new Set<string>(), id = '', updated = 0;
  return async (raw: string): Promise<string | null> => {
    if (!raw.startsWith('GMBQR1:')) return raw;
    if (raw.length > 2331) throw new Error('QR frame too large');
    const next = raw.split(':', 2)[1];
    if (next !== id || Date.now() - updated > 300_000) { frames = new Set(); id = next; }
    updated = Date.now();
    if (frames.has(raw)) return null;
    if (frames.size >= 64) throw new Error('Too many QR frames');
    frames.add(raw);
    try {
      const result = await join([...frames]);
      if (result !== null) frames.clear();
      return result;
    } catch (error) { frames.clear(); throw error; }
  };
}
