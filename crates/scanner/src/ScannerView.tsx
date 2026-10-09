import {
  useEffect,
  useMemo,
  useRef,
  type CSSProperties,
} from 'react';

import {
  ScannerController,
  ScannerFailure,
  type ScannerControllerLike,
} from './scanner';

export interface ScannerViewProps {
  active?: boolean;
  controller?: ScannerControllerLike;
  className?: string;
  style?: CSSProperties;
  onRawValue: (raw: string) => void;
  onReady?: () => void;
  onFailure?: (failure: ScannerFailure) => void;
}

/**
 * 统一 React 摄像头预览组件。
 *
 * 组件不附带产品样式、不解析 QR_V1；每个业务页面自行决定允许的码型和错误反馈。
 */
export function ScannerView({
  active = true,
  controller: injectedController,
  className,
  style,
  onRawValue,
  onReady,
  onFailure,
}: ScannerViewProps) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const callbacksRef = useRef({ onRawValue, onReady, onFailure });
  const controller = useMemo(
    () => injectedController ?? new ScannerController(),
    [injectedController],
  );

  useEffect(() => {
    callbacksRef.current = { onRawValue, onReady, onFailure };
  }, [onFailure, onRawValue, onReady]);

  useEffect(() => {
    const video = videoRef.current;
    if (!active || video === null) {
      controller.stop();
      return;
    }

    void controller
      .start(video, {
        onRawValue: (raw) => callbacksRef.current.onRawValue(raw),
        onReady: () => callbacksRef.current.onReady?.(),
      })
      .catch((error: unknown) => {
        const failure =
          error instanceof ScannerFailure
            ? error
            : new ScannerFailure('operation-failed', '启动扫码失败', error);
        callbacksRef.current.onFailure?.(failure);
      });
    return () => controller.stop();
  }, [active, controller]);

  return (
    <video
      ref={videoRef}
      className={className}
      style={style}
      muted
      playsInline
    />
  );
}
