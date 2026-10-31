import { otherTabsApi } from './api';
import { sanitizeError } from "../client/tauri";
import { LocalDocViewer } from "./LocalDocViewer";
import { RuntimeConstitutionViewer } from "../guard/RuntimeConstitutionViewer";
import type { LocalDoc, OtherTabsPayload } from "./types";
import { useEffect, useState } from 'react';

// 正文仅在当前程序会话缓存；并发挂载共享请求，失败后允许重试。
let whitepaper: LocalDoc | null = null;
let whitepaperRequest: Promise<LocalDoc> | null = null;
function loadWhitepaper(): Promise<LocalDoc> {
  if (whitepaper) return Promise.resolve(whitepaper);
  if (!whitepaperRequest) {
    whitepaperRequest = otherTabsApi.getWhitepaperDocument()
      .then((data): LocalDoc => {
        whitepaper = { key: 'whitepaper', markdown: data.markdown };
        return whitepaper;
      })
      .finally(() => { whitepaperRequest = null; });
  }
  return whitepaperRequest;
}

type Props = {
  activeKey: 'whitepaper' | 'party' | 'constitution';
};

export function OtherTabsSection({ activeKey }: Props) {
  const [payload, setPayload] = useState<OtherTabsPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [whitepaperDoc, setWhitepaperDoc] = useState<LocalDoc | null>(whitepaper);
  const [whitepaperError, setWhitepaperError] = useState<string | null>(null);
  const [whitepaperAttempt, setWhitepaperAttempt] = useState(0);

  useEffect(() => {
    if (activeKey !== 'whitepaper') return undefined;
    let cancelled = false;
    setWhitepaperDoc(whitepaper);
    setWhitepaperError(null);
    void loadWhitepaper()
      .then((data) => {
        if (!cancelled) setWhitepaperDoc(data);
      })
      .catch((e) => {
        if (!cancelled) setWhitepaperError(sanitizeError(e));
      });
    return () => { cancelled = true; };
  }, [activeKey, whitepaperAttempt]);

  useEffect(() => {
    if (activeKey === 'whitepaper') {
      return undefined;
    }

    let cancelled = false;
    void otherTabsApi.getOtherTabsContent()
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
  }, [activeKey]);

  // 白皮书正文来自官网；目录和正文仍使用节点原有本地渲染器。
  if (activeKey === 'whitepaper') {
    return (
      <section className="section other-tab-section" key="whitepaper">
        {whitepaperError ? (
          <div className="runtime-constitution-message">
            <pre className="error">{whitepaperError}</pre>
            <button type="button" onClick={() => setWhitepaperAttempt((value) => value + 1)}>重试</button>
          </div>
        ) : whitepaperDoc ? (
          <LocalDocViewer doc={whitepaperDoc} />
        ) : (
          <p>白皮书加载中...</p>
        )}
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

  return (
    <section className="section other-tab-section" key={tab.key}>
      {tab.contentType === 'runtimeConstitution' ? (
        <RuntimeConstitutionViewer />
      ) : (
        <p>{tab.text}</p>
      )}
    </section>
  );
}
