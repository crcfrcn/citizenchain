import { useEffect, useId, useRef, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { MetricHelp } from '../desktop/MetricHelp';

export type TransferProposalStep = 'form' | 'qr' | 'submit' | 'done' | 'error';

// 按业务名称统一配置，现有及后续提案表单共用同一说明入口。
const PROPOSAL_HELP: Record<string, { title: string; items: readonly string[] }> = {
  '转账': {
    title: '转账提案说明',
    items: [
      '用于从本机构主账户向收款账户转账，支持公民链地址和以太坊地址。',
      '管理员扫码签名后提交提案，由本机构有资格的管理员进行内部投票。',
      '国家储委会赞成票达到13票、省储委会或省储行达到6票，则投票通过。',
      '转账本金由主账户支付，金额手续费由本机构费用账户支付。',
      '投票通过后由链上执行，是否到账以实际执行结果为准。',
    ],
  },
  '手续费划转': {
    title: '手续费划转提案说明',
    items: [
      '用于将本机构费用账户中的资金划转至本机构主账户，收款账户固定，不可更改。',
      '管理员扫码签名后提交提案，由本机构管理员进行内部投票。',
      '单次划转本金不得超过可用余额的80%；可用余额按账户余额扣除链上最低保留余额计算。',
      '划转本金和金额手续费均由费用账户支付，扣除后仍须满足链上余额保留要求。',
      '投票通过后由链上检查余额和额度并执行，以实际执行结果为准。',
    ],
  },
  '安全基金转账': {
    title: '安全基金转账提案说明',
    items: [
      '用于从国家储委会安全基金账户向收款账户转账，支持公民链地址和以太坊地址。',
      '由具有相应权限的国家储委会管理员发起，经本机构内部投票，赞成票达到13票则通过。',
      '转账本金由安全基金账户支付，金额手续费由国家储委会费用账户支付。',
      '投票通过后仍须满足链上账户支出和余额要求，是否到账以实际执行结果为准。',
    ],
  },
  '两和基金转账': {
    title: '两和基金转账提案说明',
    items: [
      '用于从国家储委会两和基金账户向收款账户转账，支持公民链地址和以太坊地址。',
      '由具有相应权限的国家储委会管理员发起，经本机构内部投票，赞成票达到13票则通过。',
      '转账本金由两和基金账户支付，金额手续费由国家储委会费用账户支付。',
      '投票通过后仍须满足链上账户支出和余额要求，是否到账以实际执行结果为准。',
    ],
  },
  '验证密钥': {
    title: '验证密钥说明',
    items: [
      '用于更换本机构节点参与GRANDPA最终性验证的密钥。',
      '正常更换由有权限的委员发起，同时验证旧、新密钥的签名证明，无须内部投票。',
      '旧私钥丢失或无法签名时，使用紧急恢复，经本机构管理员内部投票通过后调度更换，不使用联合投票。',
      '新密钥按链上规定延迟生效；切换确认前，本机保留旧、新两把密钥。',
      '最终性确认新密钥生效后，本机自动清理旧私钥并重启节点。',
    ],
  },
  '协议升级': {
    title: '协议升级提案说明',
    items: [
      '用于更新全链公民协议，即公民链Runtime运行时协议。',
      '由具有相应权限的国家储委会或省储委会管理员发起，提交升级理由、Runtime WASM文件等参数。',
      '提案进入全体治理机构的联合投票，符合转入条件时进入联合公投。',
      '投票通过后由链上执行升级，配套参数按链上规定生效；执行是否成功以链上结果为准。',
      '新版公民协议须遵守节点程序的宪法守卫和节点守卫，否则节点将拒绝新版公民协议。',
    ],
  },
  '决议发行': {
    title: '决议发行提案说明',
    items: [
      '用于通过全体治理机构的联合投票决议新增发行公民币。',
      '由具有相应权限的国家储委会或省储委会管理员发起，提交发行理由、发行总额及收款分配明细。',
      '收款账户须与链上允许的省储委会账户集合一致，各项分配金额合计必须等于发行总额。',
      '提案进入全体治理机构的联合投票，符合转入条件时进入联合公投。',
      '投票通过后由链上按分配方案执行发行，是否发行到账以实际执行结果为准。',
    ],
  },
  '决议销毁': {
    title: '决议销毁提案说明',
    items: [
      '用于按本机构内部投票决议销毁本机构账户中的公民币。',
      '由具有相应权限的国家储委会、省储委会或省储行管理员发起，经本机构内部投票，国家储委会赞成票达到13票、省储委会或省储行达到6票则通过。',
      '销毁金额从指定的本机构账户中扣除并减少全链发行总量，销毁后不可恢复。',
      '金额手续费由本机构费用账户支付，执行后机构账户仍须满足链上最低余额要求。',
      '投票通过后由链上执行销毁，是否成功以实际执行结果为准。',
    ],
  },
};

type Props = {
  title: string;
  institutionName: string;
  step: TransferProposalStep;
  generating: boolean;
  signDisabled: boolean;
  onCancel: () => void;
  onSign: () => void;
  onComplete: () => void;
  onRetry?: () => void;
  completeLabel?: string;
  scanner?: ReactNode;
  children: ReactNode;
};

export function TransferProposalDialog({
  title, institutionName, step, generating, signDisabled,
  onCancel, onSign, onComplete, onRetry, completeLabel = '完成', scanner, children,
}: Props) {
  const titleId = useId();
  const dialogRef = useRef<HTMLDialogElement>(null);
  const titleRef = useRef<HTMLHeadingElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const wasScannerOpen = useRef(false);
  const scannerOpen = Boolean(scanner);
  const help = PROPOSAL_HELP[title.replace(/^发起/, '').replace(/提案$/, '')];

  useEffect(() => {
    const dialog = dialogRef.current;
    dialog?.showModal();
    return () => dialog?.close();
  }, []);

  useEffect(() => {
    bodyRef.current?.scrollTo?.(0, 0);
    titleRef.current?.focus({ preventScroll: true });
  }, [step]);

  useEffect(() => {
    if (scannerOpen) {
      dialogRef.current?.querySelector<HTMLButtonElement>('.address-scan-modal button')?.focus();
    } else if (wasScannerOpen.current) {
      dialogRef.current?.querySelector<HTMLButtonElement>('.scan-icon-btn')?.focus();
    }
    wasScannerOpen.current = scannerOpen;
  }, [scannerOpen]);

  return createPortal(
    <dialog
      ref={dialogRef}
      className="institution-transfer-dialog"
      data-step={step}
      aria-labelledby={titleId}
      onCancel={(event) => event.preventDefault()}
    >
      <div className="institution-transfer-panel" inert={scannerOpen}>
        <header>
          <div className="institution-proposal-title">
            <h2 id={titleId} ref={titleRef} tabIndex={-1}>{title}</h2>
            {help && <MetricHelp title={help.title} items={help.items} />}
          </div>
          <p className="proposal-institution-name">{institutionName}</p>
        </header>
        <div ref={bodyRef} className="institution-transfer-body" aria-busy={generating || step === 'submit'}>
          {children}
        </div>
        {step !== 'submit' && (
          <footer className="institution-transfer-actions">
            {step !== 'done' && (
              <button type="button" className="institution-transfer-cancel" onClick={onCancel} disabled={generating}>取消</button>
            )}
            {step === 'form' && (
              <button type="button" className="institution-transfer-primary" onClick={onSign} disabled={signDisabled || generating}>
                {generating ? '生成中…' : '签名'}
              </button>
            )}
            {step === 'error' && onRetry && (
              <button type="button" className="institution-transfer-primary" onClick={onRetry}>重试</button>
            )}
            {step === 'done' && (
              <button type="button" className="institution-transfer-primary" onClick={onComplete}>{completeLabel}</button>
            )}
          </footer>
        )}
      </div>
      {scanner}
    </dialog>,
    document.body,
  );
}
