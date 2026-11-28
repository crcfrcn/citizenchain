// 导出React扫码设备接口，并提供由调用方验真的传输帧组装助手。

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
export {SigningQr, QrSession, createQrAssembler, loadQrTransport, type QrProfile} from './qr';
