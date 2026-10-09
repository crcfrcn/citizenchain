// @vitest-environment jsdom

import { render, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ScannerView } from '../src/ScannerView';
import {
  ScannerFailure,
  type ScannerCallbacks,
  type ScannerControllerLike,
} from '../src/scanner';

describe('ScannerView', () => {
  it('启用时启动统一控制器，停用和卸载时停止但不释放注入设备', async () => {
    const controller = new FakeController();
    const onRawValue = vi.fn();
    const view = render(
      <ScannerView controller={controller} onRawValue={onRawValue} />,
    );

    await waitFor(() => expect(controller.start).toHaveBeenCalledOnce());
    expect(controller.video).toBeInstanceOf(HTMLVideoElement);
    controller.callbacks?.onRawValue('raw-value');
    expect(onRawValue).toHaveBeenCalledWith('raw-value');

    view.rerender(
      <ScannerView
        active={false}
        controller={controller}
        onRawValue={onRawValue}
      />,
    );
    expect(controller.stop).toHaveBeenCalled();
    view.unmount();
    expect(controller.dispose).not.toHaveBeenCalled();
  });

  it('控制器启动失败统一交给 onFailure', async () => {
    const failure = new ScannerFailure(
      'camera-unavailable',
      '摄像头不可用',
    );
    const controller = new FakeController(failure);
    const onFailure = vi.fn();
    render(
      <ScannerView
        controller={controller}
        onRawValue={() => {}}
        onFailure={onFailure}
      />,
    );

    await waitFor(() => expect(onFailure).toHaveBeenCalledWith(failure));
  });
});

class FakeController implements ScannerControllerLike {
  readonly start = vi.fn(
    async (video: HTMLVideoElement, callbacks: ScannerCallbacks) => {
      this.video = video;
      this.callbacks = callbacks;
      if (this.failure !== undefined) throw this.failure;
    },
  );
  readonly stop = vi.fn();
  readonly dispose = vi.fn();
  video?: HTMLVideoElement;
  callbacks?: ScannerCallbacks;

  constructor(private readonly failure?: ScannerFailure) {}
}
