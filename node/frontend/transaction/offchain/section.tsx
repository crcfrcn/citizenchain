// 清算行 tab 主入口。视图状态机驱动机构查看/节点声明流程。
//
// 状态机:
//   empty                        列表 + ＋添加清算行
//   add-input-cid               输入 cid_number 自动 debounce 搜索 CID 候选(不带"查询"按钮)
//   check-multisig               链上查 Institutions[cid_number]
//                                  ├─ 已存在 → institution-detail
//                                  └─ 不存在 → 提示去 onchina 控制台创建机构(节点不承接创建)
//   institution-detail           机构详情 = 卡片栅格 + 折叠卡片(其他账户 / 管理员) + 节点信息
//                                顶部按钮:未声明节点 → declare-node;已声明 → 内联展示节点信息
//   other-accounts-list          子页:其他账户列表
//   admin-list                   子页:管理员列表
//   declare-node                 多签 Active 但本机未声明节点 → 填 RPC + 自测 + 签名声明

import { useEffect, useState, useCallback } from 'react';
import { sanitizeError } from '../../tauri';
import { institutionReadApi } from './institution/api';
import { ClearingBankAddPage } from './institution/add-candidate';
import { ClearingBankInstitutionDetailPage } from './institution/institution-detail';
import { OtherAccountsListPage } from './institution/other-accounts';
import type {
  AccountWithBalance,
  EligibleClearingBankCandidate,
  InstitutionDetail,
} from './institution/types';
import { offchainApi } from './api';
import type { ClearingBankView } from './types';
import { ClearingBankDeclareNodePage } from './node-register';
import { ClearingBankAdminListPage } from './admin-unlock';
import './styles.css';

const STORAGE_KEY = 'gmb-clearing-bank-known-cids';

type KnownCidEntry = { cidNumber: string; cidFullName: string };

function loadKnownCids(): KnownCidEntry[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (e): e is KnownCidEntry =>
        typeof e === 'object' && e !== null
        && typeof (e as { cidNumber?: unknown }).cidNumber === 'string'
        && typeof (e as { cidFullName?: unknown }).cidFullName === 'string',
    );
  } catch {
    return [];
  }
}

/// 把已确认链上存在(Institutions[cid_number] 已存在)的机构入条目加入本机已添加列表。
/// **不要**在用户只是选了候选时调本函数,否则若链上没有,EmptyView 会显示孤儿卡。
export function saveKnownCid(entry: KnownCidEntry) {
  const list = loadKnownCids().filter((e) => e.cidNumber !== entry.cidNumber);
  list.unshift(entry);
  localStorage.setItem(STORAGE_KEY, JSON.stringify(list.slice(0, 50)));
}

/// 从本机已添加列表中删除指定 cid_number 条目(链上判定为 None 或用户主动移除时调)。
function removeKnownCid(cidNumber: string) {
  const list = loadKnownCids().filter((e) => e.cidNumber !== cidNumber);
  localStorage.setItem(STORAGE_KEY, JSON.stringify(list.slice(0, 50)));
}

export function ClearingBankSection() {
  const [view, setView] = useState<ClearingBankView>({ kind: 'empty' });
  const [knownCids, setKnownCids] = useState<KnownCidEntry[]>(() => loadKnownCids());

  const goEmpty = useCallback(() => {
    setKnownCids(loadKnownCids());
    setView({ kind: 'empty' });
  }, []);

  const goAdd = useCallback(() => setView({ kind: 'add-input-cid' }), []);

  const goCheckMultisig = useCallback((cidNumber: string, cidFullName: string) => {
    // **不在此处** saveKnownCid。用户只是选了候选,链上未必存在。
    // 只有 goInstitutionDetail(链上确认存在)才调 saveKnownCid。
    setView({ kind: 'check-multisig', cidNumber, cidFullName });
  }, []);

  const goInstitutionDetail = useCallback((cidNumber: string, cidFullName?: string) => {
    if (cidFullName) {
      saveKnownCid({ cidNumber, cidFullName });
      setKnownCids(loadKnownCids());
    }
    setView({ kind: 'institution-detail', cidNumber });
  }, []);

  return (
    <div className="governance-section clearing-bank-section">
      {view.kind === 'empty' && (
        <EmptyView
          knownCids={knownCids}
          onAdd={goAdd}
          onOpen={(s) => goInstitutionDetail(s.cidNumber, s.cidFullName)}
        />
      )}

      {view.kind === 'add-input-cid' && (
        <ClearingBankAddPage
          onBack={goEmpty}
          onSelectCandidate={(c) => goCheckMultisig(c.cidNumber, c.cidFullName)}
          onSelectKnownCid={(s) => goCheckMultisig(s, '')}
        />
      )}

      {view.kind === 'check-multisig' && (
        <CheckMultisigView
          cidNumber={view.cidNumber}
          cidFullName={view.cidFullName}
          onBack={goEmpty}
          onExists={() => goInstitutionDetail(view.cidNumber, view.cidFullName)}
        />
      )}

      {view.kind === 'institution-detail' && (
        <ClearingBankInstitutionDetailPage
          cidNumber={view.cidNumber}
          onBack={goEmpty}
          onOpenOtherAccounts={(detail) =>
            setView({
              kind: 'other-accounts-list',
              cidNumber: view.cidNumber,
              otherAccounts: detail.otherAccounts,
            })
          }
          onOpenAdminList={(detail) =>
            setView({
              kind: 'admin-list',
              cidNumber: view.cidNumber,
              admins: detail.admins,
              threshold: detail.threshold,
              adminsLen: detail.adminsLen,
            })
          }
          onDeclareNode={(cidNumber, cidFullName) =>
            setView({ kind: 'declare-node', cidNumber, cidFullName })
          }
        />
      )}

      {view.kind === 'other-accounts-list' && (
        <OtherAccountsListPage
          cidNumber={view.cidNumber}
          otherAccounts={view.otherAccounts}
          onBack={() => setView({ kind: 'institution-detail', cidNumber: view.cidNumber })}
        />
      )}

      {view.kind === 'admin-list' && (
        <ClearingBankAdminListPage
          cidNumber={view.cidNumber}
          admins={view.admins}
          threshold={view.threshold}
          adminsLen={view.adminsLen}
          onBack={() => setView({ kind: 'institution-detail', cidNumber: view.cidNumber })}
        />
      )}

      {view.kind === 'declare-node' && (
        <ClearingBankDeclareNodePage
          cidNumber={view.cidNumber}
          cidFullName={view.cidFullName}
          onBack={goEmpty}
          onSuccess={() => goInstitutionDetail(view.cidNumber, view.cidFullName)}
        />
      )}
    </div>
  );
}

// ─── 子组件:空视图 + 已添加列表 ───
//
// 挂载时自愈本地脏数据 ——— 对每个 knownCid 条目调
// fetchInstitutionDetail(cidNumber);链上 None 即从 localStorage 移除。
//
// 链查失败(网络/节点未运行)时**保留**条目(避免误删合法记录)。
function EmptyView({
  knownCids: initialKnown,
  onAdd,
  onOpen,
}: {
  knownCids: KnownCidEntry[];
  onAdd: () => void;
  onOpen: (entry: KnownCidEntry) => void;
}) {
  const [knownCids, setKnownCids] = useState<KnownCidEntry[]>(initialKnown);

  useEffect(() => {
    let cancelled = false;
    const cleanup = async () => {
      const next: KnownCidEntry[] = [];
      for (const entry of initialKnown) {
        try {
          const detail = await institutionReadApi.fetchInstitutionDetail(entry.cidNumber);
          if (detail !== null) {
            next.push(entry); // 链上有(Pending / Active 都保留)
          } else {
            removeKnownCid(entry.cidNumber); // 链上明确不存在 → 移除孤儿
          }
        } catch {
          next.push(entry); // 链查失败保守保留
        }
      }
      if (!cancelled) setKnownCids(next);
    };
    cleanup();
    return () => {
      cancelled = true;
    };
  }, [initialKnown]);

  return (
    <>
      <div className="admin-list-header">
        <h2>清算行</h2>
        <button className="primary-button" onClick={onAdd}>+ 添加清算行</button>
      </div>
      {knownCids.length > 0 && (
        <div className="admin-grid">
          {knownCids.map((s) => (
            <div
              key={s.cidNumber}
              className="metric-card admin-card"
              role="button"
              tabIndex={0}
              onClick={() => onOpen(s)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') onOpen(s);
              }}
            >
              <span className="admin-card-index">→</span>
              <div>
                <strong>{s.cidFullName}</strong>
                <code className="admin-card-address" style={{ marginLeft: 8 }}>{s.cidNumber}</code>
              </div>
            </div>
          ))}
        </div>
      )}
    </>
  );
}

// ─── 子组件:链上查机构是否已存在,已存在跳详情,不存在提示去 onchina 创建 ───
function CheckMultisigView({
  cidNumber,
  cidFullName,
  onBack,
  onExists,
}: {
  cidNumber: string;
  cidFullName: string;
  onBack: () => void;
  onExists: () => void;
}) {
  const [error, setError] = useState<string | null>(null);
  const [missing, setMissing] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    setMissing(false);
    institutionReadApi
      .fetchInstitutionDetail(cidNumber)
      .then((detail) => {
        if (cancelled) return;
        if (detail) onExists();
        else setMissing(true);
      })
      .catch((e) => {
        if (!cancelled) setError(sanitizeError(e));
      });
    return () => {
      cancelled = true;
    };
  }, [cidNumber, onExists]);

  return (
    <>
      <button className="back-button" onClick={onBack}>← 返回</button>
      {error ? (
        <div className="error">{error}</div>
      ) : missing ? (
        <div className="no-data">
          {cidFullName || cidNumber} 链上尚未创建机构多签。机构创建已迁至 onchina
          控制台,请由对应机构管理员在 onchina 完成创建并经投票生效后,再回本页声明清算行节点。
        </div>
      ) : (
        <p>正在判定 {cidFullName || cidNumber} 链上多签状态…</p>
      )}
    </>
  );
}

// AccountWithBalance 来自 types(被 view kind 中的字段引用,确保 import 有效)。
export type _Reexport = AccountWithBalance | EligibleClearingBankCandidate | InstitutionDetail;
