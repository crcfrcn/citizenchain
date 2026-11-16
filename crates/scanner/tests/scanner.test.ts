// @vitest-environment jsdom

import { describe, expect, it, vi } from 'vitest';
import { createQrAssembler } from '../src/index';

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
      onRawValue: (raw) => { detected.push(raw); },
      onReady: () => readyCount += 1,
    });
    expect(readyCount).toBe(0);
    expect(harness.frames).toHaveLength(1);

    harness.frames[0]?.(0);
    expect(readyCount).toBe(1);
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
    HAVE_CURRENT_DATA: 2,
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
    expect(h.video.play).toHaveBeenCalledOnce(); expect(h.frames).toHaveLength(1);
    h.frames[0]?.(0);
    expect(ready).toHaveBeenCalledOnce();
    h.controller.stop(); h.controller.stop(); expect(h.track.stop).toHaveBeenCalledOnce();
  });

  it('首帧超时释放媒体流，迟到的解码帧不能恢复旧会话', async () => {
    vi.useFakeTimers();
    const harness = createHarness(() => ({ data: 'late' }));
    const ready = vi.fn();
    const found = vi.fn();
    const failed = vi.fn();
    try {
      Object.defineProperty(harness.video, 'readyState', { value: 0, configurable: true });
      await harness.controller.start(harness.video, { onRawValue: found, onReady: ready, onFailure: failed });
      harness.frames[0]?.(0);
      await vi.advanceTimersByTimeAsync(10_000);
      expect(failed).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ kind: 'camera-unavailable' }));
      expect(harness.track.stop).toHaveBeenCalledOnce();
      Object.defineProperty(harness.video, 'readyState', { value: 2 });
      harness.frames[harness.frames.length - 1]?.(11_000);
      expect(ready).not.toHaveBeenCalled();
      expect(found).not.toHaveBeenCalled();
    } finally {
      harness.controller.stop();
      vi.useRealTimers();
    }
  });

  it.each(['mute', 'ended'])('视频流 %s 后释放摄像头并只报告一次', async (eventName) => {
    const track = Object.assign(new EventTarget(), { kind: 'video', readyState: 'live', stop: vi.fn() });
    const video = fakeVideo();
    const failed = vi.fn();
    const frames: FrameRequestCallback[] = [];
    const controller = new ScannerController({
      getUserMedia: async () => ({ getTracks: () => [track] }) as unknown as MediaStream,
      createCanvas: () => fakeCanvas(fakeContext()),
      requestFrame: (callback) => frames.push(callback),
      cancelFrame: vi.fn(),
      isSecureContext: () => true,
      decode: () => null,
    });
    await controller.start(video, { onRawValue: vi.fn(), onFailure: failed });
    frames[0]?.(0);
    track.dispatchEvent(new Event(eventName));
    track.dispatchEvent(new Event('ended'));
    expect(failed).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ kind: 'camera-unavailable' }));
    expect(track.stop).toHaveBeenCalledOnce();
    expect(video.srcObject).toBeNull();
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

// 多帧只改变设备结果门控；仍使用同一摄像头与原250毫秒扫描间隔。
describe('连续扫码与异步会话', () => {
  it('明确未收齐继续同一设备，完成只交付一次并停止', async () => {
    const h = createHarness(() => ({ data: 'frame' }));
    const found = vi.fn().mockReturnValueOnce(false);
    await h.controller.start(h.video, { onRawValue: found });
    h.frames[0]?.(0);
    expect(h.track.stop).not.toHaveBeenCalled(); expect(h.frames).toHaveLength(2);
    h.frames[1]?.(250);
    expect(found).toHaveBeenCalledTimes(2); expect(h.track.stop).toHaveBeenCalledOnce();
    h.frames[1]?.(500); expect(found).toHaveBeenCalledTimes(2);
    expect(h.video.play).toHaveBeenCalledOnce();
  });

  it('等待组帧期间不重入，缺帧返回后才继续扫描', async () => {
    const h = createHarness(() => ({ data: 'frame' }));
    let finish!: (result: void | false) => void;
    const found = vi.fn(() => new Promise<void | false>(resolve => { finish = resolve; }));
    await h.controller.start(h.video, { onRawValue: found });
    h.frames[0]?.(0); h.frames[0]?.(250);
    expect(found).toHaveBeenCalledOnce(); expect(h.frames).toHaveLength(1);
    expect(h.track.stop).not.toHaveBeenCalled();
    finish(false); await Promise.resolve();
    expect(h.frames).toHaveLength(2);
    h.frames[1]?.(500); expect(found).toHaveBeenCalledTimes(2);
    finish(); await Promise.resolve(); expect(h.track.stop).toHaveBeenCalledOnce();
  });

  it.each([false, undefined] as const)('取消后的迟到结果%s不会恢复摄像头', async result => {
    const h = createHarness(() => ({ data: 'frame' }));
    let finish!: (result: void | false) => void;
    const found = vi.fn(() => new Promise<void | false>(resolve => { finish = resolve; }));
    await h.controller.start(h.video, { onRawValue: found }); h.frames[0]?.(0);
    h.controller.stop(); finish(result); await Promise.resolve();
    expect(h.track.stop).toHaveBeenCalledOnce(); expect(h.frames).toHaveLength(1);
    h.frames[0]?.(500); expect(found).toHaveBeenCalledOnce();
  });

  it('旧异步失败不能停止新会话或向新会话报告错误', async () => {
    const oldTrack = { stop: vi.fn() }, nextTrack = { stop: vi.fn() };
    const media = vi.fn().mockResolvedValueOnce({ getTracks: () => [oldTrack] }).mockResolvedValueOnce({ getTracks: () => [nextTrack] });
    const frames: FrameRequestCallback[] = []; const failed = vi.fn();
    let reject!: (error: Error) => void;
    const controller = new ScannerController({ getUserMedia: media, isSecureContext: () => true, createCanvas: () => fakeCanvas(fakeContext()),
      requestFrame: callback => { frames.push(callback); return frames.length; }, cancelFrame: vi.fn(), decode: () => ({ data: 'frame' }) });
    await controller.start(fakeVideo(), { onRawValue: () => new Promise<void>((_, fail) => { reject = fail; }), onFailure: failed });
    frames[0]?.(0); controller.stop();
    await controller.start(fakeVideo(), { onRawValue: vi.fn(), onFailure: failed });
    reject(new Error('old request failed')); await Promise.resolve();
    expect(failed).not.toHaveBeenCalled(); expect(nextTrack.stop).not.toHaveBeenCalled();
    frames[1]?.(250); expect(nextTrack.stop).toHaveBeenCalledOnce();
    expect(oldTrack.stop).toHaveBeenCalledOnce();
  });

  it.each(['sync', 'async'])('%s处理失败只报告一次且释放摄像头', async mode => {
    const h = createHarness(() => ({ data: 'frame' })); const failed = vi.fn();
    const error = new Error('invalid frame');
    await h.controller.start(h.video, { onRawValue: () => { if (mode === 'sync') throw error; return Promise.reject(error); }, onFailure: failed });
    h.frames[0]?.(0); await Promise.resolve();
    expect(failed).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ kind: 'operation-failed', cause: error }));
    expect(h.track.stop).toHaveBeenCalledOnce(); expect(h.frames).toHaveLength(1);
    h.frames[0]?.(500); expect(failed).toHaveBeenCalledOnce();
  });

  it('通过公开组帧助手收齐后才交付业务结果', async () => {
    const raws = ['GMBQR1:a:0:2:first', 'GMBQR1:a:1:2:second']; let index = 0;
    const h = createHarness(() => ({ data: raws[index++]! })); const delivered = vi.fn();
    const join = vi.fn(async (frames: string[]) => frames.length === 2 ? 'complete payload' : null);
    const assemble = createQrAssembler(join);
    await h.controller.start(h.video, { onRawValue: async (raw): Promise<void | false> => {
      const complete = await assemble(raw); if (complete === null) return false; delivered(complete);
    } });
    h.frames[0]?.(0); await vi.waitFor(() => expect(h.frames).toHaveLength(2));
    expect(delivered).not.toHaveBeenCalled(); expect(h.track.stop).not.toHaveBeenCalled();
    h.frames[1]?.(250); await vi.waitFor(() => expect(h.track.stop).toHaveBeenCalledOnce());
    expect(delivered).toHaveBeenCalledExactlyOnceWith('complete payload'); expect(h.video.play).toHaveBeenCalledOnce();
  });
});

describe('公开传输帧助手', () => {
  const frame = (id: string, n: number) => `GMBQR1:${id}:${n}:64:payload`;
  it('普通码原样交付，重复帧不重复请求，完成后清理缓存', async () => {
    const join = vi.fn(async (frames: string[]) => frames.length === 2 ? 'complete' : null);
    const assemble = createQrAssembler(join);
    expect(await assemble('ordinary QR')).toBe('ordinary QR'); expect(join).not.toHaveBeenCalled();
    const a = frame('a', 1), b = frame('a', 0);
    expect(await assemble(a)).toBeNull(); expect(await assemble(a)).toBeNull(); expect(join).toHaveBeenCalledOnce();
    expect(await assemble(b)).toBe('complete'); expect(join).toHaveBeenLastCalledWith([a, b]);
    expect(await assemble(a)).toBeNull(); expect(join).toHaveBeenLastCalledWith([a]);
  });
  it('批次切换与五分钟超时清理旧帧', async () => {
    const now = vi.spyOn(Date, 'now'); now.mockReturnValue(0);
    try {
      const join = vi.fn(async () => null); const assemble = createQrAssembler(join);
      await assemble(frame('a', 0)); await assemble(frame('b', 0));
      expect(join).toHaveBeenLastCalledWith([frame('b', 0)]);
      now.mockReturnValue(300_001); await assemble(frame('b', 1));
      expect(join).toHaveBeenLastCalledWith([frame('b', 1)]);
    } finally { now.mockRestore(); }
  });
  it('组帧失败清理缓存后重新收集', async () => {
    const join = vi.fn(async () => null).mockRejectedValueOnce(new Error('bad frame'));
    const assemble = createQrAssembler(join);
    await expect(assemble(frame('a', 0))).rejects.toThrow('bad frame');
    await assemble(frame('a', 1)); expect(join).toHaveBeenLastCalledWith([frame('a', 1)]);
  });
  it('保留64帧和2331字符边界，越界不交给Rust入口', async () => {
    const join = vi.fn(async () => null), assemble = createQrAssembler(join);
    for (let n = 0; n < 64; n++) expect(await assemble(frame('a', n))).toBeNull();
    expect(join).toHaveBeenCalledTimes(64);
    await expect(assemble(frame('a', 64))).rejects.toThrow('Too many QR frames');
    expect(join).toHaveBeenCalledTimes(64);
    const sized = createQrAssembler(join), max = 'GMBQR1:b:' + 'x'.repeat(2331 - 'GMBQR1:b:'.length);
    await sized(max); expect(join).toHaveBeenCalledTimes(65);
    await expect(sized(max + 'x')).rejects.toThrow('QR frame too large'); expect(join).toHaveBeenCalledTimes(65);
  });
});
