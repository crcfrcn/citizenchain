// @vitest-environment jsdom

import { describe, expect, it, vi } from 'vitest';

import {
  ScannerController,
  ScannerFailure,
  type QrDecoder,
} from '../src/scanner';

describe('ScannerController', () => {
  it('摄像头帧严格经 canvas 和 jsQR，首次识别后停止设备', async () => {
    const harness = createHarness(() => ({ data: ' QR_V1 ' }));
    const detected: string[] = [];
    let readyCount = 0;

    await harness.controller.start(harness.video, {
      onRawValue: (raw) => detected.push(raw),
      onReady: () => readyCount += 1,
    });
    expect(readyCount).toBe(1);
    expect(harness.frames).toHaveLength(1);

    harness.frames[0]?.(0);
    expect(detected).toEqual(['QR_V1']);
    expect(harness.context.drawImage).toHaveBeenCalledOnce();
    expect(harness.context.getImageData).toHaveBeenCalledOnce();
    expect(harness.track.stop).toHaveBeenCalledOnce();
    expect(harness.video.srcObject).toBeNull();

    harness.frames[0]?.(1);
    expect(detected).toEqual(['QR_V1']);
  });

  it('停止发生在摄像头异步打开之前时，迟到的媒体轨道也会释放', async () => {
    let resolveStream: ((stream: MediaStream) => void) | undefined;
    const pending = new Promise<MediaStream>((resolve) => {
      resolveStream = resolve;
    });
    const track = { stop: vi.fn() };
    const controller = new ScannerController({
      getUserMedia: () => pending,
      isSecureContext: () => true,
    });
    const video = fakeVideo();

    const starting = controller.start(video, { onRawValue: () => {} });
    controller.stop();
    resolveStream?.({ getTracks: () => [track] } as unknown as MediaStream);
    await starting;

    expect(track.stop).toHaveBeenCalledOnce();
    expect(video.play).not.toHaveBeenCalled();
  });

  it('相册图片与摄像头复用同一 canvas 和 jsQR 解码器', async () => {
    const decoder = vi.fn<QrDecoder>(() => ({ data: 'image-value' }));
    const context = fakeContext();
    const bitmap = {
      width: 40,
      height: 30,
      close: vi.fn(),
    } as unknown as ImageBitmap;
    const controller = new ScannerController({
      decode: decoder,
      createCanvas: () => fakeCanvas(context),
      createImageBitmap: async () => bitmap,
      isSecureContext: () => true,
    });

    const raw = await controller.scanImage(
      new File(['image'], 'qr.png', { type: 'image/png' }),
    );

    expect(raw).toBe('image-value');
    expect(context.drawImage).toHaveBeenCalledOnce();
    expect(context.getImageData).toHaveBeenCalledOnce();
    expect(decoder).toHaveBeenCalledOnce();
    expect(bitmap.close).toHaveBeenCalledOnce();
  });

  it('无二维码与摄像头权限拒绝映射为稳定错误类型', async () => {
    const context = fakeContext();
    const bitmap = {
      width: 20,
      height: 20,
      close: vi.fn(),
    } as unknown as ImageBitmap;
    const noQrController = new ScannerController({
      decode: () => null,
      createCanvas: () => fakeCanvas(context),
      createImageBitmap: async () => bitmap,
      isSecureContext: () => true,
    });
    await expect(
      noQrController.scanImage(
        new File(['image'], 'empty.png', { type: 'image/png' }),
      ),
    ).rejects.toMatchObject({ kind: 'no-qr-code' });

    const deniedController = new ScannerController({
      getUserMedia: async () => {
        throw new DOMException('denied', 'NotAllowedError');
      },
      isSecureContext: () => true,
    });
    await expect(
      deniedController.start(fakeVideo(), { onRawValue: () => {} }),
    ).rejects.toEqual(
      expect.objectContaining<Partial<ScannerFailure>>({
        kind: 'permission-denied',
        message: '无法取得摄像头访问权限。请检查系统授权；若刚安装或更新程序，请完全退出后重新打开。',
        cause: expect.objectContaining({ name: 'NotAllowedError', message: 'denied' }),
      }),
    );
  });

  it('非图片和释放后的调用从严拒绝', async () => {
    const controller = new ScannerController({ isSecureContext: () => true });
    await expect(
      controller.scanImage(new File(['text'], 'qr.txt', { type: 'text/plain' })),
    ).rejects.toMatchObject({ kind: 'operation-failed' });

    controller.dispose();
    expect(() => controller.dispose()).not.toThrow();
    await expect(
      controller.start(fakeVideo(), { onRawValue: () => {} }),
    ).rejects.toMatchObject({ kind: 'disposed' });
  });
});

function createHarness(decode: QrDecoder) {
  const frames: FrameRequestCallback[] = [];
  const track = { stop: vi.fn() };
  const stream = { getTracks: () => [track] } as unknown as MediaStream;
  const context = fakeContext();
  const video = fakeVideo();
  const controller = new ScannerController({
    decode,
    getUserMedia: async () => stream,
    createCanvas: () => fakeCanvas(context),
    requestFrame: (callback) => {
      frames.push(callback);
      return frames.length;
    },
    cancelFrame: vi.fn(),
    isSecureContext: () => true,
  });
  return { controller, context, frames, track, video };
}

function fakeCanvas(context: CanvasRenderingContext2D): HTMLCanvasElement {
  return {
    width: 0,
    height: 0,
    getContext: vi.fn(() => context),
  } as unknown as HTMLCanvasElement;
}

function fakeContext(): CanvasRenderingContext2D {
  return {
    drawImage: vi.fn(),
    getImageData: vi.fn(
      () =>
        ({
          data: new Uint8ClampedArray(16),
          width: 2,
          height: 2,
        }) as ImageData,
    ),
  } as unknown as CanvasRenderingContext2D;
}

function fakeVideo(): HTMLVideoElement {
  return {
    HAVE_ENOUGH_DATA: 4,
    readyState: 4,
    videoWidth: 2,
    videoHeight: 2,
    srcObject: null,
    play: vi.fn(async () => {}),
    pause: vi.fn(),
  } as unknown as HTMLVideoElement;
}

// 摄像头资源与时间边界由真实控制器处理，假设备只提供可控异步点。
describe('ScannerController 生命周期边界', () => {
  it('不安全上下文在请求设备之前拒绝，已运行时重复start不重复预览', async () => {
    const media = vi.fn();
    const insecure = new ScannerController({ isSecureContext: () => false, getUserMedia: media });
    await expect(insecure.start(fakeVideo(), { onRawValue: vi.fn() })).rejects.toMatchObject({ kind: 'insecure-context' });
    expect(media).not.toHaveBeenCalled();
    const h = createHarness(() => null); const ready = vi.fn();
    await h.controller.start(h.video, { onRawValue: vi.fn(), onReady: ready });
    await h.controller.start(h.video, { onRawValue: vi.fn(), onReady: ready });
    expect(h.video.play).toHaveBeenCalledOnce(); expect(ready).toHaveBeenCalledOnce(); expect(h.frames).toHaveLength(1);
    h.controller.stop(); h.controller.stop(); expect(h.track.stop).toHaveBeenCalledOnce();
  });

  it('节流边界为250毫秒，停止后的旧帧不继续解码', async () => {
    const decode = vi.fn<QrDecoder>(() => null); const h = createHarness(decode);
    await h.controller.start(h.video, { onRawValue: vi.fn() });
    for (const time of [0, 100, 249, 250, 499, 500]) h.frames[h.frames.length - 1]?.(time);
    expect(decode).toHaveBeenCalledTimes(3);
    h.controller.stop(); h.frames[h.frames.length - 1]?.(750); expect(decode).toHaveBeenCalledTimes(3);
  });

  it('视频未就绪和单帧读取异常均不产生结果，后续好帧仍可识别', async () => {
    const decode = vi.fn<QrDecoder>(() => ({ data: 'next-frame' })); const h = createHarness(decode); const found = vi.fn();
    Object.defineProperty(h.video, 'readyState', { value: 0, configurable: true });
    await h.controller.start(h.video, { onRawValue: found }); h.frames[h.frames.length - 1]?.(0);
    expect(decode).not.toHaveBeenCalled();
    Object.defineProperty(h.video, 'readyState', { value: 4, configurable: true });
    vi.mocked(h.context.getImageData).mockImplementationOnce(() => { throw new Error('temporary read error'); });
    h.frames[h.frames.length - 1]?.(250); expect(found).not.toHaveBeenCalled();
    h.frames[h.frames.length - 1]?.(500); expect(found).toHaveBeenCalledExactlyOnceWith('next-frame'); expect(h.track.stop).toHaveBeenCalledOnce();
  });

  it('释放时仍在等待媒体设备，迟到流被释放且不启动播放', async () => {
    let finish!: (stream: MediaStream) => void;
    const track = { stop: vi.fn() }; const video = fakeVideo();
    const c = new ScannerController({ isSecureContext: () => true, getUserMedia: () => new Promise(resolve => { finish = resolve; }) });
    const pending = c.start(video, { onRawValue: vi.fn() }); c.dispose();
    finish({ getTracks: () => [track] } as unknown as MediaStream); await pending;
    expect(track.stop).toHaveBeenCalledOnce(); expect(video.play).not.toHaveBeenCalled();
    await expect(c.scanImage(new File([], 'qr.png', { type: 'image/png' }))).rejects.toMatchObject({ kind: 'disposed' });
  });

  it('画布不可用时释放已经取得的摄像头，不保留半启动状态', async () => {
    const track = { stop: vi.fn() }; const video = fakeVideo();
    const c = new ScannerController({ isSecureContext: () => true, getUserMedia: async () => ({ getTracks: () => [track] }) as unknown as MediaStream, createCanvas: () => ({ getContext: () => null }) as unknown as HTMLCanvasElement });
    await expect(c.start(video, { onRawValue: vi.fn() })).rejects.toMatchObject({ kind: 'unsupported' });
    expect(track.stop).toHaveBeenCalledOnce(); expect(video.srcObject).toBeNull();
  });

  it.each([['SecurityError', 'permission-denied'], ['NotFoundError', 'camera-unavailable'], ['NotReadableError', 'camera-unavailable'], ['OverconstrainedError', 'camera-unavailable']])('设备%s映射为%s且保留原原因', async (name, kind) => {
    const failure = new DOMException('synthetic-device-failure', name);
    const c = new ScannerController({ isSecureContext: () => true, getUserMedia: async () => { throw failure; } });
    await expect(c.start(fakeVideo(), { onRawValue: vi.fn() })).rejects.toMatchObject({ kind, cause: failure });
  });

  it('相册各失败路径均关闭位图，后缀图片可走同一识别路径', async () => {
    for (const failure of ['empty', 'draw', 'canvas'] as const) {
      const bitmap = { width: 2, height: 2, close: vi.fn() }; const context = fakeContext();
      if (failure === 'draw') vi.mocked(context.drawImage).mockImplementation(() => { throw new Error('synthetic draw failure'); });
      const c = new ScannerController({ decode: () => null, createImageBitmap: async () => bitmap as unknown as ImageBitmap, createCanvas: () => failure === 'canvas' ? ({ getContext: () => null }) as unknown as HTMLCanvasElement : fakeCanvas(context) });
      await expect(c.scanImage(new File([], 'qr.PNG'))).rejects.toMatchObject({ kind: failure === 'empty' ? 'no-qr-code' : failure === 'canvas' ? 'unsupported' : 'operation-failed' });
      expect(bitmap.close).toHaveBeenCalledOnce();
    }
  });

  it('旧播放完成不能终止停止后重新启动的新摄像头', async () => {
    let finishPlay!: () => void; const oldVideo = fakeVideo(); const newVideo = fakeVideo();
    vi.mocked(oldVideo.play).mockImplementationOnce(() => new Promise<void>(resolve => { finishPlay = resolve; }));
    const oldTrack = { stop: vi.fn() }, newTrack = { stop: vi.fn() };
    const getUserMedia = vi.fn().mockResolvedValueOnce({ getTracks: () => [oldTrack] }).mockResolvedValueOnce({ getTracks: () => [newTrack] });
    const c = new ScannerController({ getUserMedia, isSecureContext: () => true, createCanvas: () => fakeCanvas(fakeContext()), requestFrame: () => 1, cancelFrame: vi.fn(), decode: () => null });
    const old = c.start(oldVideo, { onRawValue: vi.fn() }); await Promise.resolve();
    c.stop(); await c.start(newVideo, { onRawValue: vi.fn() });
    finishPlay(); await old;
    expect(newTrack.stop).not.toHaveBeenCalled(); expect(newVideo.srcObject).not.toBeNull();
    c.dispose(); expect(newTrack.stop).toHaveBeenCalledOnce();
  });
});

it('默认jsQR真实解码摄像头与图片的同一静态二维码金标', async () => {
  const { qrImage } = await import('./qr');
  const image = qrImage(); const context = fakeContext(); vi.mocked(context.getImageData).mockReturnValue(image);
  const bitmap = { width: image.width, height: image.height, close: vi.fn() };
  const track = { stop: vi.fn() }; const frames: FrameRequestCallback[] = []; const video = fakeVideo();
  Object.defineProperty(video, 'videoWidth', { value: image.width }); Object.defineProperty(video, 'videoHeight', { value: image.height });
  // 不注入decode，明确使用生产默认的jsQR。
  const controller = new ScannerController({ isSecureContext: () => true, getUserMedia: async () => ({ getTracks: () => [track] }) as unknown as MediaStream, createCanvas: () => fakeCanvas(context), createImageBitmap: async () => bitmap as unknown as ImageBitmap, requestFrame: callback => { frames.push(callback); return frames.length; }, cancelFrame: vi.fn() });
  expect(await controller.scanImage(new File([], 'fixture.png', { type: 'image/png' }))).toBe('QR_V1');
  expect(bitmap.close).toHaveBeenCalledOnce();
  const found = vi.fn(); await controller.start(video, { onRawValue: found }); frames[0]?.(0);
  expect(found).toHaveBeenCalledExactlyOnceWith('QR_V1'); expect(track.stop).toHaveBeenCalledOnce(); controller.dispose();
});


it.each([[false, false], [false, true], [true, true]])('半启动重启(stop=%s，同video=%s)只清理旧设备', async (stopBefore, sameVideo) => {
  const oldVideo = fakeVideo(), newVideo = sameVideo ? oldVideo : fakeVideo();
  let finish!: () => void;
  vi.mocked(oldVideo.play).mockImplementationOnce(() => new Promise<void>(resolve => { finish = resolve; }));
  const oldTrack = { stop: vi.fn() }, nextTrack = { stop: vi.fn() };
  const oldStream = { getTracks: () => [oldTrack] }, nextStream = { getTracks: () => [nextTrack] };
  const media = vi.fn().mockResolvedValueOnce(oldStream).mockResolvedValueOnce(nextStream);
  const c = new ScannerController({ isSecureContext: () => true, getUserMedia: media, createCanvas: () => fakeCanvas(fakeContext()), requestFrame: () => 1, cancelFrame: vi.fn(), decode: () => null });
  const pending = c.start(oldVideo, { onRawValue: vi.fn() }); await Promise.resolve();
  if (stopBefore) c.stop();
  await c.start(newVideo, { onRawValue: vi.fn() });
  expect(oldTrack.stop).toHaveBeenCalledOnce(); finish(); await pending;
  expect(nextTrack.stop).not.toHaveBeenCalled(); expect(newVideo.srcObject).toBe(nextStream);
  c.dispose(); expect(nextTrack.stop).toHaveBeenCalledOnce();
});
