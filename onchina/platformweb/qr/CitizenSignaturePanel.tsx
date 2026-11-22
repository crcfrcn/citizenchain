// 展示待签二维码并扫描签名响应，统一摄像头生命周期。

import { useState, useMemo, useEffect, useRef, type ReactNode } from 'react';
import { verifiedQrTransportFrames } from './citizenQr';
import { ScannerView, createQrAssembler } from '@gmb/scanner-react';
import { Button, QRCode, Typography } from 'antd';
import { ScanOutlined } from '@ant-design/icons';

export interface CitizenSignaturePanelProps {
  qrTitle?: string;
  /** 仅机构钱包流程启用现有原生分帧，其他调用保持原展示。 */
  framed?: boolean;
  scannerTitle?: string;
  qrValue?: string | null;
  qrPlaceholderValue?: string;
  qrHint?: ReactNode;
  scannerHint?: ReactNode;
  primaryActionText?: string;
  primaryActionLoading?: boolean;
  primaryActionDisabled?: boolean;
  onPrimaryAction?: () => void;
  scannerButtonText?: string;
  scannerActiveText?: string;
  scannerDisabled?: boolean;
  scannerLoading?: boolean;
  onDetected: (raw: string) => void | Promise<void>;
  onScannerError?: (message: string) => void;
}

import { request } from '../workspace/http';

const cornerColor = '#0d9488';

export function CitizenSignaturePanel({
  qrTitle = '签名二维码',
  framed = false,
  scannerTitle = '扫码窗口',
  qrValue,
  qrPlaceholderValue = 'CID_SIGN_PENDING',
  qrHint,
  scannerHint = '开启摄像头扫描已签名的二维码',
  primaryActionText,
  primaryActionLoading = false,
  primaryActionDisabled = false,
  onPrimaryAction,
  scannerButtonText = '开启扫码',
  scannerActiveText = '停止扫码',
  scannerDisabled = false,
  scannerLoading = false,
  onDetected,
  onScannerError,
}: CitizenSignaturePanelProps) {
  const [scannerActive, setScannerActive] = useState(false);
  const [scannerReady, setScannerReady] = useState(false);
  const session = useRef(0);
  useEffect(() => () => { session.current += 1; }, []);
  const assemble = useMemo(() => createQrAssembler(frames => request<string | null>('/api/qr/join', {method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(frames)})), [scannerActive]);

  const [frameSource, setFrameSource] = useState<string | null>(null);
  const [frames, setFrames] = useState<string[]>([]);
  const [frameError, setFrameError] = useState<string | null>(null);
  const [frameIndex, setFrameIndex] = useState(0);
  const [frameAttempt, setFrameAttempt] = useState(0);
  const [paused, setPaused] = useState(false);
  const needsFrames = !!(framed && qrValue && new TextEncoder().encode(qrValue).length > 2331);
  useEffect(() => {
    setFrameSource(qrValue ?? null); setFrames([]); setFrameIndex(0); setFrameError(null); setPaused(false);
    if (!needsFrames || !qrValue) return;
    const controller = new AbortController();
    request<unknown>('/api/qr/split', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(qrValue), signal: controller.signal })
      .then(value => { if (!controller.signal.aborted) setFrames(verifiedQrTransportFrames(qrValue, value)); })
      .catch(error => { if (!controller.signal.aborted) setFrameError(error instanceof Error ? error.message : String(error)); });
    return () => controller.abort();
  }, [qrValue, needsFrames, frameAttempt]);
  useEffect(() => {
    if (frames.length < 2 || paused) return;
    const timer = setInterval(() => setFrameIndex(index => (index + 1) % frames.length), 600);
    return () => clearInterval(timer);
  }, [frames, paused, qrValue]);
  const activeFrames = frameSource === qrValue ? frames : [];
  const visibleFrameError = frameSource === qrValue && needsFrames ? frameError : null;
  const displayReady = !!qrValue && (!needsFrames || activeFrames.length > 0) && !visibleFrameError;
  const currentQrValue = needsFrames ? activeFrames[frameIndex] ?? qrPlaceholderValue : qrValue || qrPlaceholderValue;

  return (
    <div style={{ display: 'flex', gap: 32, alignItems: 'stretch', flexWrap: 'wrap' }}>
      <div
        style={{
          flex: '1 1 300px',
          minWidth: 280,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
        }}
      >
        <Typography.Text strong style={{ fontSize: 14, marginBottom: 16, color: '#374151' }}>
          {qrTitle}
        </Typography.Text>
        <div
          style={{
            position: 'relative',
            width: 260,
            height: 260,
            background: '#f8fffe',
            borderRadius: 16,
            border: '2px solid #e6f7f5',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            overflow: 'hidden',
          }}
        >
          <div style={{ position: 'absolute', top: 0, left: 0, width: 20, height: 20, borderTop: `3px solid ${cornerColor}`, borderLeft: `3px solid ${cornerColor}`, borderTopLeftRadius: 8 }} />
          <div style={{ position: 'absolute', top: 0, right: 0, width: 20, height: 20, borderTop: `3px solid ${cornerColor}`, borderRight: `3px solid ${cornerColor}`, borderTopRightRadius: 8 }} />
          <div style={{ position: 'absolute', bottom: 0, left: 0, width: 20, height: 20, borderBottom: `3px solid ${cornerColor}`, borderLeft: `3px solid ${cornerColor}`, borderBottomLeftRadius: 8 }} />
          <div style={{ position: 'absolute', bottom: 0, right: 0, width: 20, height: 20, borderBottom: `3px solid ${cornerColor}`, borderRight: `3px solid ${cornerColor}`, borderBottomRightRadius: 8 }} />
          <div
            style={{
              filter: displayReady ? 'none' : 'blur(3px) opacity(0.4)',
              transition: 'filter 0.3s ease',
            }}
          >
            <QRCode value={currentQrValue} size={228} color="#134e4a" />
          </div>
        </div>
        <div style={{ marginTop: 14, textAlign: 'center' }}>
          {needsFrames && !activeFrames.length && !visibleFrameError && <Typography.Paragraph>正在准备签名二维码</Typography.Paragraph>}
          {visibleFrameError && <><Typography.Paragraph type="danger">{visibleFrameError}</Typography.Paragraph><Button onClick={() => setFrameAttempt(value => value + 1)}>重新载入二维码</Button></>}
          {activeFrames.length > 1 && <div style={{ marginBottom: 12 }}><Typography.Paragraph>第 {frameIndex + 1} / {activeFrames.length} 帧，请持续扫码至钱包完成合并</Typography.Paragraph>
            <Button size="small" onClick={() => setFrameIndex(index => (index + frames.length - 1) % frames.length)}>上一帧</Button>
            <Button size="small" onClick={() => setPaused(value => !value)}>{paused ? '继续轮播' : '暂停轮播'}</Button>
            <Button size="small" onClick={() => setFrameIndex(index => (index + 1) % frames.length)}>下一帧</Button></div>}
          <Typography.Text type="secondary" style={{ fontSize: 12, display: 'block', marginBottom: 12 }}>
            {qrHint}
          </Typography.Text>
          {primaryActionText ? (
            <Button
              type="primary"
              size="large"
              onClick={onPrimaryAction}
              loading={primaryActionLoading}
              disabled={primaryActionDisabled}
              style={{
                borderRadius: 10,
                fontWeight: 500,
                width: 200,
                boxShadow: '0 2px 8px rgba(13,148,136,0.3)',
              }}
            >
              {primaryActionText}
            </Button>
          ) : null}
        </div>
      </div>

      <div
        style={{
          width: 1,
          background: 'linear-gradient(to bottom, transparent, #e5e7eb, transparent)',
          alignSelf: 'stretch',
          margin: '0 4px',
        }}
      />

      <div
        style={{
          flex: '1 1 300px',
          minWidth: 280,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
        }}
      >
        <Typography.Text strong style={{ fontSize: 14, marginBottom: 16, color: '#374151' }}>
          {scannerTitle}
        </Typography.Text>
        <div
          style={{
            width: 260,
            height: 260,
            background: 'linear-gradient(145deg, #0f172a, #1e293b)',
            borderRadius: 16,
            overflow: 'hidden',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            position: 'relative',
            border: '2px solid #334155',
            boxShadow: 'inset 0 2px 8px rgba(0,0,0,0.3)',
          }}
        >
          <div style={{ position: 'absolute', top: 8, left: 8, width: 16, height: 16, borderTop: `2px solid ${cornerColor}`, borderLeft: `2px solid ${cornerColor}`, borderTopLeftRadius: 4, zIndex: 2 }} />
          <div style={{ position: 'absolute', top: 8, right: 8, width: 16, height: 16, borderTop: `2px solid ${cornerColor}`, borderRight: `2px solid ${cornerColor}`, borderTopRightRadius: 4, zIndex: 2 }} />
          <div style={{ position: 'absolute', bottom: 8, left: 8, width: 16, height: 16, borderBottom: `2px solid ${cornerColor}`, borderLeft: `2px solid ${cornerColor}`, borderBottomLeftRadius: 4, zIndex: 2 }} />
          <div style={{ position: 'absolute', bottom: 8, right: 8, width: 16, height: 16, borderBottom: `2px solid ${cornerColor}`, borderRight: `2px solid ${cornerColor}`, borderBottomRightRadius: 4, zIndex: 2 }} />
          <ScannerView
            active={scannerActive}
            style={{ width: '100%', height: '100%', objectFit: 'cover' }}
            onReady={() => setScannerReady(true)}
            onRawValue={async (raw): Promise<void | false> => {
              const current = session.current;
              try {
                const complete = await assemble(raw);
                if (current !== session.current) return;
                if (complete === null) return false;
                setScannerReady(false);
                setScannerActive(false);
                void onDetected(complete);
              } catch (error) {
                if (current !== session.current) return;
                setScannerReady(false);
                setScannerActive(false);
                onScannerError?.(String(error));
              }
            }}
            onFailure={(failure) => {
              session.current += 1;
              setScannerReady(false);
              setScannerActive(false);
              onScannerError?.(failure.message);
            }}
          />
          {!scannerReady ? (
            <div
              style={{
                position: 'absolute',
                inset: 0,
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'center',
                justifyContent: 'center',
                gap: 8,
              }}
            >
              <ScanOutlined style={{ color: 'rgba(255,255,255,0.25)', fontSize: 32 }} />
              <Typography.Text style={{ color: 'rgba(255,255,255,0.5)', fontSize: 12 }}>
                {scannerActive ? '摄像头初始化中...' : '等待开启摄像头'}
              </Typography.Text>
            </div>
          ) : null}
        </div>
        <div style={{ marginTop: 14, textAlign: 'center' }}>
          <Typography.Text type="secondary" style={{ fontSize: 12, display: 'block', marginBottom: 12 }}>
            {scannerHint}
          </Typography.Text>
          <Button
            size="large"
            onClick={() => {
              session.current += 1;
              setScannerReady(false);
              setScannerActive((value) => !value);
            }}
            disabled={scannerDisabled || !!(needsFrames && !displayReady)}
            loading={scannerLoading}
            style={{ borderRadius: 10, fontWeight: 500, width: 200 }}
          >
            {scannerActive ? scannerActiveText : scannerButtonText}
          </Button>
        </div>
      </div>
    </div>
  );
}
