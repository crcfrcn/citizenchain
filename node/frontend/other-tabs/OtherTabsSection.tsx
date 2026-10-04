import { useEffect, useState } from 'react';
import { sanitizeError } from '../tauri';
import { LOCAL_DOCS } from '../local-docs.generated';
import { otherTabsApi as api } from './api';
import { LocalDocViewer } from './LocalDocViewer';
import { RuntimeConstitutionViewer } from './RuntimeConstitutionViewer';
import type { OtherTabsPayload } from './types';

type Props = {
  activeKey: 'whitepaper' | 'party' | 'constitution';
};

export function OtherTabsSection({ activeKey }: Props) {
  const [payload, setPayload] = useState<OtherTabsPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const whitepaperDoc = activeKey === 'whitepaper' ? LOCAL_DOCS.find((doc) => doc.key === 'whitepaper') : null;

  useEffect(() => {
    if (whitepaperDoc) {
      return undefined;
    }

    let cancelled = false;
    void api
      .getOtherTabsContent()
      .then((data) => {
        if (cancelled) return;
        setPayload(data);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(sanitizeError(e));
      });

    return () => {
      cancelled = true;
    };
  }, [whitepaperDoc]);

  // 白皮书是本地内置文档，不能被 Tauri/RPC 状态阻塞；公民宪法仍由 runtime API 读取。
  if (whitepaperDoc) {
    return (
      <section className="section other-tab-section" key={whitepaperDoc.key}>
        <LocalDocViewer doc={whitepaperDoc} />
      </section>
    );
  }

  if (error) {
    return <pre className="error">{error}</pre>;
  }

  if (!payload) {
    return (
      <section className="section">
        <p>加载中...</p>
      </section>
    );
  }

  const tab = payload.tabs.find((item) => item.key === activeKey);
  if (!tab) {
    return (
      <section className="section">
        <p>暂无内容</p>
      </section>
    );
  }

  // 本地文档以当前 tab 为绑定源，避免字段缺失时误回退到白皮书。
  const localDoc =
    tab.contentType === 'document'
      ? LOCAL_DOCS.find((doc) => doc.key === tab.key)
      : null;

  return (
    <section className="section other-tab-section" key={tab.key}>
      {tab.contentType === 'document' ? (
        localDoc ? (
          <LocalDocViewer doc={localDoc} />
        ) : (
          <pre className="error">文档配置错误：{activeKey}</pre>
        )
      ) : tab.contentType === 'runtimeConstitution' ? (
        <RuntimeConstitutionViewer />
      ) : (
        <p>{tab.text}</p>
      )}
    </section>
  );
}
