// 在浏览器模拟环境验证扫码预览组件及控制器交互。

// @vitest-environment jsdom

import { render, waitFor, fireEvent, act } from '@testing-library/react';
import {StrictMode} from 'react';
import { describe, expect, it, vi } from 'vitest';

import { ScannerView } from '../src/ScannerView';
import {SigningQr, QrSession, loadQrTransport} from '../src/qr';
import {
  ScannerFailure,
  type ScannerCallbacks,
  type ScannerControllerLike,
} from '../src/scanner';

describe('ScannerView', () => {
  it('严格模式下绿色播放每800毫秒切换，暂停和单帧查看有效且卸载释放资源', async () => {
    await loadQrTransport();
    vi.useFakeTimers();
    const next=vi.spyOn(QrSession.prototype,'next'),close=vi.spyOn(QrSession.prototype,'close');
    const view=render(<StrictMode><SigningQr value={'public-sign-response'.repeat(100)}/></StrictMode>);
    try{
      await act(async()=>{for(let n=0;n<6;n++)await Promise.resolve()});
      expect(view.container.querySelector('svg path[fill="#007A74"]')).not.toBeNull();
      expect(next).toHaveBeenCalledTimes(1);
      act(()=>vi.advanceTimersByTime(799));expect(next).toHaveBeenCalledTimes(1);
      act(()=>vi.advanceTimersByTime(1));expect(next).toHaveBeenCalledTimes(2);
      const second=view.container.querySelector('svg')!.innerHTML;
      fireEvent.click(view.getByRole('button',{name:'暂停'}));
      act(()=>vi.advanceTimersByTime(1600));expect(next).toHaveBeenCalledTimes(2);
      fireEvent.click(view.getByRole('button',{name:'下一帧'}));expect(next).toHaveBeenCalledTimes(3);
      fireEvent.click(view.getByRole('button',{name:'上一帧'}));
      expect(view.container.querySelector('svg')!.innerHTML).toBe(second);
      view.unmount();act(()=>vi.runOnlyPendingTimers());
      expect(next).toHaveBeenCalledTimes(3);expect(close.mock.calls.length).toBeGreaterThanOrEqual(2);
    }finally{view.unmount();next.mockRestore();close.mockRestore();vi.useRealTimers();}
  });
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

it('更新回调使用最新函数，不重启设备；换控制器只停止旧控制器', async () => {
  const old = new FakeController(); const next = new FakeController(); const first = vi.fn(), latest = vi.fn();
  const view = render(<ScannerView controller={old} onRawValue={first} />);
  await waitFor(() => expect(old.start).toHaveBeenCalledOnce());
  view.rerender(<ScannerView controller={old} onRawValue={latest} />);
  old.callbacks?.onRawValue('new-callback'); expect(latest).toHaveBeenCalledExactlyOnceWith('new-callback'); expect(first).not.toHaveBeenCalled(); expect(old.start).toHaveBeenCalledOnce();
  view.rerender(<ScannerView controller={next} onRawValue={latest} />);
  await waitFor(() => expect(next.start).toHaveBeenCalledOnce()); expect(old.stop).toHaveBeenCalled();
  view.unmount(); expect(next.stop).toHaveBeenCalled(); expect(old.dispose).not.toHaveBeenCalled(); expect(next.dispose).not.toHaveBeenCalled();
});

it('未知启动错误转为统一错误对象，active=false不请求启动', async () => {
  const failure = new Error('synthetic failure'); const controller = new FakeController(); controller.start.mockRejectedValue(failure);
  const onFailure = vi.fn(); const view = render(<ScannerView active={false} controller={controller} onRawValue={vi.fn()} onFailure={onFailure} />);
  expect(controller.start).not.toHaveBeenCalled();
  view.rerender(<ScannerView controller={controller} onRawValue={vi.fn()} onFailure={onFailure} />);
  await waitFor(() => expect(onFailure).toHaveBeenCalledWith(expect.objectContaining({ kind: 'operation-failed', cause: failure })));
  view.unmount();
});

it('异步未收齐结果原样传给控制器，不重启或重建设备', async () => {
  const controller = new FakeController(); let finish!: (result: false) => void;
  const pending = new Promise<false>(resolve => { finish = resolve; });
  const onRawValue = vi.fn(() => pending);
  const view = render(<ScannerView controller={controller} onRawValue={onRawValue} />);
  await waitFor(() => expect(controller.start).toHaveBeenCalledOnce());
  const returned = controller.callbacks?.onRawValue('partial frame');
  expect(returned).toBe(pending); finish(false); await expect(returned).resolves.toBe(false);
  expect(controller.start).toHaveBeenCalledOnce(); expect(controller.stop).not.toHaveBeenCalled();
  view.unmount(); expect(controller.stop).toHaveBeenCalledOnce();
});
