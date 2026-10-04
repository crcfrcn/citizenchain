// 机构码→中文标签单一真源:后端 /api/public/cid/labels 下发(primitives code.rs 的 104 码),
// 取代原前端硬编码 INSTITUTION_CODE_LABEL。模块级缓存 + 一次拉取(免登录),多组件挂载复用同一份。

import { useEffect, useState } from 'react';
import { publicRequest } from '../utils/http';

export type InstitutionCodeLabelMap = Record<string, string>;

type CidLabelsResponse = {
  institution_labels: { institution_code: string; institution_code_label: string }[];
};

let cache: InstitutionCodeLabelMap | null = null;
let inflight: Promise<InstitutionCodeLabelMap> | null = null;

function loadInstitutionCodeLabels(): Promise<InstitutionCodeLabelMap> {
  if (cache) return Promise.resolve(cache);
  if (!inflight) {
    inflight = publicRequest<CidLabelsResponse>('/api/public/cid/labels', { method: 'GET' })
      .then((data) => {
        const map: InstitutionCodeLabelMap = {};
        for (const item of data.institution_labels) {
          map[item.institution_code] = item.institution_code_label;
        }
        cache = map;
        return map;
      })
      .catch((err) => {
        inflight = null; // 失败不缓存,允许下次重试
        throw err;
      });
  }
  return inflight;
}

/**
 * 返回机构码→中文标签映射。数据到达前为空对象,消费方按 `map[code] || code` 兜底显示裸码;
 * 拉取完成后模块级缓存,后续组件挂载即同步命中。拉取失败时保持空映射(全兜底裸码),不阻断页面。
 */
export function useInstitutionCodeLabels(): InstitutionCodeLabelMap {
  const [labels, setLabels] = useState<InstitutionCodeLabelMap>(cache ?? {});
  useEffect(() => {
    if (cache) {
      setLabels(cache);
      return;
    }
    let alive = true;
    loadInstitutionCodeLabels()
      .then((map) => {
        if (alive) setLabels(map);
      })
      .catch(() => {
        /* 兜底裸码,已在渲染处处理 */
      });
    return () => {
      alive = false;
    };
  }, []);
  return labels;
}
