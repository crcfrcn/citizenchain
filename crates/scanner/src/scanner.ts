import jsQR from 'jsqr';

const scanIntervalMs = 250;

/** 统一二维码解码器签名；正式环境固定由 jsQR 实现。 */
export type QrDecoder = (
  data: Uint8ClampedArray,
  width: number,
  height: number,
  options: { inversionAttempts: 'attemptBoth' },
) => { data: string } | null;

export type ScannerFailureKind =
  | 'insecure-context'
  | 'unsupported'
  | 'permission-denied'
  | 'camera-unavailable'
  | 'no-qr-code'
  | 'disposed'
  | 'operation-failed';

/** 统一扫码设备错误；二维码码型不符属于产品业务页面职责。 */
export class ScannerFailure extends Error {
  readonly kind: ScannerFailureKind;
  readonly cause?: unknown;

  constructor(kind: ScannerFailureKind, message: string, cause?: unknown) {
    super(message);
    this.name = 'ScannerFailure';
    this.kind = kind;
    this.cause = cause;
  }
}

export interface ScannerCallbacks {
  onRawValue: (raw: string) => void;
  onReady?: () => void;
}

/**
 * 依赖注入口只用于包测试和特殊宿主；业务页面不得注入第二套二维码解码器。
 */
export interface ScannerControllerDependencies {
  decode?: QrDecoder;
  getUserMedia?: (constraints: MediaStreamConstraints) => Promise<MediaStream>;
  createCanvas?: () => HTMLCanvasElement;
  createImageBitmap?: (image: ImageBitmapSource) => Promise<ImageBitmap>;
  requestFrame?: (callback: FrameRequestCallback) => number;
  cancelFrame?: (handle: number) => void;
  isSecureContext?: () => boolean;
}

/** React 组件所需的最小控制器接口，便于产品 Widget 测试注入假设备。 */
export interface ScannerControllerLike {
  start(video: HTMLVideoElement, callbacks: ScannerCallbacks): Promise<void>;
  stop(): void;
  dispose(): void;
}

/**
 * 浏览器扫码设备控制器。
 *
 * 摄像头和图片识别都严格使用 `canvas -> ImageData -> jsQR`。本类不引用
 * 浏览器原生检测 API，只负责设备生命周期、首次结果门控和统一错误。
 */
export class ScannerController implements ScannerControllerLike {
  private readonly decode: QrDecoder;
  private readonly getUserMedia: (
    constraints: MediaStreamConstraints,
  ) => Promise<MediaStream>;
  private readonly createCanvas: () => HTMLCanvasElement;
  private readonly createBitmap: (
    image: ImageBitmapSource,
  ) => Promise<ImageBitmap>;
  private readonly requestFrame: (callback: FrameRequestCallback) => number;
  private readonly cancelFrame: (handle: number) => void;
  private readonly isSecureContext: () => boolean;

  private stream: MediaStream | null = null;
  private video: HTMLVideoElement | null = null;
  private frameHandle: number | null = null;
  private generation = 0;
  private running = false;
  private disposed = false;
  private claimed = false;

  constructor(dependencies: ScannerControllerDependencies = {}) {
    this.decode = dependencies.decode ?? (jsQR as QrDecoder);
    this.getUserMedia =
      dependencies.getUserMedia ??
      ((constraints) => {
        if (!navigator.mediaDevices?.getUserMedia) {
          throw new ScannerFailure(
            'unsupported',
            '当前运行环境不支持摄像头扫码',
          );
        }
        return navigator.mediaDevices.getUserMedia(constraints);
      });
    this.createCanvas =
      dependencies.createCanvas ?? (() => document.createElement('canvas'));
    this.createBitmap =
      dependencies.createImageBitmap ??
      ((image) => {
        if (typeof globalThis.createImageBitmap !== 'function') {
          throw new ScannerFailure(
            'unsupported',
            '当前运行环境不支持二维码图片识别',
          );
        }
        return globalThis.createImageBitmap(image);
      });
    this.requestFrame =
      dependencies.requestFrame ??
      ((callback) => globalThis.requestAnimationFrame(callback));
    this.cancelFrame =
      dependencies.cancelFrame ??
      ((handle) => globalThis.cancelAnimationFrame(handle));
    this.isSecureContext =
      dependencies.isSecureContext ?? (() => globalThis.isSecureContext);
  }

  async start(
    video: HTMLVideoElement,
    callbacks: ScannerCallbacks,
  ): Promise<void> {
    this.ensureActive();
    if (this.running) return;
    if (!this.isSecureContext()) {
      throw new ScannerFailure(
        'insecure-context',
        '当前页面不是安全环境，无法使用摄像头',
      );
    }

    const generation = ++this.generation;
    this.claimed = false;
    try {
      const stream = await this.getUserMedia({
        video: {
          facingMode: 'environment',
          width: { ideal: 1280 },
          height: { ideal: 720 },
        },
        audio: false,
      });
      if (this.disposed || generation !== this.generation) {
        stopTracks(stream);
        return;
      }

      this.stream = stream;
      this.video = video;
      video.srcObject = stream;
      await video.play();
      if (this.disposed || generation !== this.generation) {
        this.stop();
        return;
      }

      this.running = true;
      callbacks.onReady?.();
      const canvas = this.createCanvas();
      const context = getCanvasContext(canvas);
      let lastScanAt = Number.NEGATIVE_INFINITY;
      const scanFrame = (timestamp: number) => {
        if (!this.running || generation !== this.generation) return;
        if (timestamp - lastScanAt >= scanIntervalMs) {
          lastScanAt = timestamp;
          try {
            const raw = this.decodeVideoFrame(video, canvas, context);
            if (raw !== null && !this.claimed) {
              this.claimed = true;
              this.stop();
              callbacks.onRawValue(raw);
              return;
            }
          } catch {
            // 单帧读取失败不终止摄像头；下一帧继续识别。
          }
        }
        this.frameHandle = this.requestFrame(scanFrame);
      };
      this.frameHandle = this.requestFrame(scanFrame);
    } catch (error) {
      if (generation === this.generation) this.stop();
      if (error instanceof ScannerFailure) throw error;
      throw mapDeviceFailure(error, '打开摄像头失败');
    }
  }

  stop(): void {
    this.generation += 1;
    this.running = false;
    if (this.frameHandle !== null) {
      this.cancelFrame(this.frameHandle);
      this.frameHandle = null;
    }
    if (this.stream !== null) {
      stopTracks(this.stream);
      this.stream = null;
    }
    if (this.video !== null) {
      this.video.pause();
      this.video.srcObject = null;
      this.video = null;
    }
  }

  /** 识别业务页面已经选择的本地图片，只返回二维码原始字符串。 */
  async scanImage(file: File): Promise<string> {
    this.ensureActive();
    if (!isImageFile(file)) {
      throw new ScannerFailure('operation-failed', '请选择二维码图片文件');
    }

    let bitmap: ImageBitmap | null = null;
    try {
      bitmap = await this.createBitmap(file);
      const canvas = this.createCanvas();
      canvas.width = bitmap.width;
      canvas.height = bitmap.height;
      const context = getCanvasContext(canvas);
      context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
      const image = context.getImageData(0, 0, canvas.width, canvas.height);
      const raw = this.decodeRaw(image)?.trim();
      if (!raw) {
        throw new ScannerFailure('no-qr-code', '图片中未识别到二维码');
      }
      return raw;
    } catch (error) {
      if (error instanceof ScannerFailure) throw error;
      throw mapDeviceFailure(error, '二维码图片识别失败');
    } finally {
      bitmap?.close();
    }
  }

  dispose(): void {
    if (this.disposed) return;
    this.stop();
    this.disposed = true;
  }

  private decodeVideoFrame(
    video: HTMLVideoElement,
    canvas: HTMLCanvasElement,
    context: CanvasRenderingContext2D,
  ): string | null {
    if (
      video.readyState < video.HAVE_ENOUGH_DATA ||
      video.videoWidth <= 0 ||
      video.videoHeight <= 0
    ) {
      return null;
    }
    canvas.width = video.videoWidth;
    canvas.height = video.videoHeight;
    context.drawImage(video, 0, 0, canvas.width, canvas.height);
    return this.decodeRaw(
      context.getImageData(0, 0, canvas.width, canvas.height),
    )?.trim() || null;
  }

  private decodeRaw(image: ImageData): string | null {
    return (
      this.decode(image.data, image.width, image.height, {
        inversionAttempts: 'attemptBoth',
      })?.data ?? null
    );
  }

  private ensureActive(): void {
    if (this.disposed) {
      throw new ScannerFailure('disposed', '扫码设备已经释放');
    }
  }
}

function getCanvasContext(canvas: HTMLCanvasElement): CanvasRenderingContext2D {
  const context = canvas.getContext('2d', { willReadFrequently: true });
  if (context === null) {
    throw new ScannerFailure('unsupported', '当前运行环境不支持画布扫码');
  }
  return context;
}

function isImageFile(file: File): boolean {
  return (
    file.type.startsWith('image/') ||
    /\.(png|jpe?g|webp|gif|bmp)$/i.test(file.name)
  );
}

function stopTracks(stream: MediaStream): void {
  for (const track of stream.getTracks()) track.stop();
}

function mapDeviceFailure(error: unknown, message: string): ScannerFailure {
  const name = error instanceof DOMException ? error.name : '';
  if (name === 'NotAllowedError' || name === 'SecurityError') {
    return new ScannerFailure(
      'permission-denied',
      '摄像头访问被系统或用户拒绝',
      error,
    );
  }
  if (
    name === 'NotFoundError' ||
    name === 'NotReadableError' ||
    name === 'OverconstrainedError'
  ) {
    return new ScannerFailure('camera-unavailable', '摄像头不可用', error);
  }
  return new ScannerFailure('operation-failed', message, error);
}
