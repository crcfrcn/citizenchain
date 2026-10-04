// 机构列表：按 orgTypeFilter 过滤显示省储委会或省储行，点击进入详情。
import { useEffect, useState } from 'react';
import { sanitizeError } from '../tauri';
import { governanceApi as api } from './api';
import type { GovernanceOverview, InstitutionListItem } from './types';

type Props = {
  onSelect: (cidNumber: string) => void;
  /** 按机构类型过滤：1=省储委会, 2=省储行。不传则显示全部。 */
  orgTypeFilter?: number;
};

export function InstitutionListView({ onSelect, orgTypeFilter }: Props) {
  const [data, setData] = useState<GovernanceOverview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    api.getGovernanceOverview()
      .then((overview) => {
        setData(overview);
        setError(null);
      })
      .catch((e) => setError(sanitizeError(e)))
      .finally(() => setLoading(false));
  }, []);

  if (loading) {
    return <div className="governance-section"><p>加载中…</p></div>;
  }

  if (error) {
    return (
      <div className="governance-section">
        <div className="error">{error}</div>
      </div>
    );
  }

  if (!data) return null;

  // 根据 orgTypeFilter 选择要显示的机构列表
  let items: InstitutionListItem[];
  if (orgTypeFilter === 1) {
    items = data.provincialCouncils;
  } else if (orgTypeFilter === 2) {
    items = data.provincialBanks;
  } else {
    items = [
      ...data.nationalCouncils,
      ...data.provincialCouncils,
      ...data.provincialBanks,
    ];
  }

  return (
    <div className="governance-section">
      {data.warning && <div className="warning">{data.warning}</div>}
      <div className="institution-grid">
        {items.map((item) => (
          <div
            key={item.cidNumber}
            className="institution-card"
            onClick={() => onSelect(item.cidNumber)}
          >
            <div className="institution-card-name">{item.cidShortName}</div>
            <div className="institution-card-meta">
              <span className="institution-card-type">{item.orgTypeLabel}</span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
