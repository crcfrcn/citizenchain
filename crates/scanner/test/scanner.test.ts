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
        message: '摄像头访问被系统或用户拒绝',
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
