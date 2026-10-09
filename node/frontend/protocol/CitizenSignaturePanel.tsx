import type { ReactNode } from 'react';
import { QRCodeSVG } from 'qrcode.react';

import { QrScanner } from './QrScanner';

export type CitizenSignatureStatus = 'ready' | 'submitting' | 'success' | 'error';

type Props = {
  qrValue: string;
  countdownSeconds?: number;
  status?: CitizenSignatureStatus;
  statusTitle?: string;
  statusMessage?: ReactNode;
  error?: string | null;
  onScan: (responseJson: string) => void;
  onScanError: (error: string) => void;
};

function defaultStatusTitle(status: CitizenSignatureStatus): string {
  if (status === 'submitting') return '正在识别签名';
  if (status === 'success') return '签名识别成功';
  if (status === 'error') return '签名识别失败';
  return '识别签名';
}

/**
 * 公民钱包扫码签名统一面板。
 *
 * 本组件只负责“签名请求二维码 + 签名响应扫码框”的通用 UI，
 * 不解析业务载荷、不展示签名账户地址，也不提交链上交易，
 * 避免投票、转账、激活等流程耦合在一起。
 */
export function CitizenSignaturePanel({
  qrValue,
  countdownSeconds,
  status = 'ready',
  statusTitle,
  statusMessage,
  error,
  onScan,
  onScanError,
}: Props) {
  const title = statusTitle ?? defaultStatusTitle(status);

  return (
    <div className="citizen-signature-panel">
      <section className="citizen-signature-pane citizen-signature-qr-pane">
        <div className="citizen-signature-pane-head">
          <h4>扫码签名</h4>
          <p>使用 公民钱包 扫描二维码，完成离线签名。</p>
        </div>
        <div className="citizen-signature-qr-box">
          <QRCodeSVG value={qrValue} size={260} level="L" />
        </div>
        <div className="citizen-signature-meta">
          {countdownSeconds != null && (
            <span>有效时间: {countdownSeconds} 秒</span>
          )}
        </div>
      </section>

      <section className="citizen-signature-pane citizen-signature-scan-pane">
        <div className="citizen-signature-pane-head">
          <h4>识别签名</h4>
          <p>识别 公民钱包 生成的签名二维码。</p>
        </div>

        {status === 'ready' ? (
          <QrScanner onScan={onScan} onError={onScanError} />
        ) : (
          <div className={`citizen-signature-status citizen-signature-status-${status}`}>
            <strong>{title}</strong>
            {status === 'error' && error ? <p>{error}</p> : statusMessage ? <p>{statusMessage}</p> : null}
          </div>
        )}
      </section>
    </div>
  );
}
