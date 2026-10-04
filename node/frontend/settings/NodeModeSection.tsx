import { useMemo, useState } from 'react';
import { sanitizeError } from '../tauri';
import { settingsApi as api } from './api';
import type { NodeMode, NodeModeState } from './types';

type Props = {
  nodeMode: NodeModeState | null;
  onUpdated: (next: NodeModeState) => void;
};

export function NodeModeSection({ nodeMode, onUpdated }: Props) {
  const [savingMode, setSavingMode] = useState<NodeMode | null>(null);
  const [error, setError] = useState<string | null>(null);

  const effectiveModeLabel = useMemo(() => {
    if (!nodeMode) return '读取中';
    return (
      nodeMode.options.find((option) => option.mode === nodeMode.effectiveMode)?.label ??
      '归档全节点'
    );
  }, [nodeMode]);

  const selectedMode = nodeMode?.selectedMode ?? 'archive';

  const selectMode = async (mode: NodeMode) => {
    if (!nodeMode || mode === nodeMode.selectedMode) return;
    setSavingMode(mode);
    setError(null);
    try {
      const next = await api.setNodeMode(mode);
      onUpdated(next);
    } catch (e) {
      setError(sanitizeError(e));
    } finally {
      setSavingMode(null);
    }
  };

  return (
    <section className="section settings-node-mode-section">
      <div className="node-mode-panel">
        <div className="node-mode-header">
          <h2>全节点模式</h2>
          <span className="node-mode-effective">当前运行：{effectiveModeLabel}</span>
        </div>
        {nodeMode ? (
          <div className="node-mode-options" role="group" aria-label="全节点模式">
            {nodeMode.options.map((option) => {
              const selected = option.mode === selectedMode;
              const classes = [
                'node-mode-option',
                selected ? 'node-mode-option-selected' : '',
                option.implementationStatus === 'pending' ? 'node-mode-option-pending' : '',
              ]
                .filter(Boolean)
                .join(' ');

              // 这里仅选择链数据模式；聊天投递不再由区块链节点承载。
              return (
                <button
                  key={option.mode}
                  type="button"
                  className={classes}
                  aria-pressed={selected}
                  disabled={savingMode !== null || !option.enabled}
                  onClick={() => {
                    void selectMode(option.mode);
                  }}
                >
                  <span className="node-mode-option-title">
                    <span className="node-mode-option-label">{option.label}</span>
                  </span>
                  <span className="node-mode-option-description">{option.description}</span>
                </button>
              );
            })}
          </div>
        ) : (
          <p className="section-inline-hint">正在读取全节点模式...</p>
        )}
        {error ? <p className="section-inline-error">{error}</p> : null}
      </div>
    </section>
  );
}
