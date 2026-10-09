/// GMB React 产品统一二维码设备适配器。
///
/// 本包只输出二维码原始字符串，不解析 QR_V1、不判断业务码型，也不执行页面导航。
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
