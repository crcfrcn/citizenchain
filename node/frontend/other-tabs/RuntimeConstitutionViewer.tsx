import { useCallback, useEffect, useState } from 'react';
import { sanitizeError } from '../tauri';
import { otherTabsApi as api } from './api';
import type { RuntimeConstitutionDocument } from './types';

export function RuntimeConstitutionViewer() {
  const [doc, setDoc] = useState<RuntimeConstitutionDocument | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    setError(null);
    void api
      .getRuntimeConstitutionDocument()
      .then(setDoc)
      .catch((e) => {
        setDoc(null);
        setError(sanitizeError(e));
      });
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  if (error) {
    return (
      <section className="runtime-constitution-shell runtime-constitution-message">
        <pre className="error">{error}</pre>
        <button type="button" onClick={load}>重试</button>
      </section>
    );
  }

  if (!doc) {
    return (
      <section className="runtime-constitution-shell runtime-constitution-message">
        <p>加载中...</p>
      </section>
    );
  }

  return (
    <section className="runtime-constitution-shell">
      {/* runtime 宪法 HTML 只在隔离 iframe 内运行目录脚本，不开放 allow-same-origin。 */}
      <iframe
        className="runtime-constitution-frame"
        title="公民宪法"
        sandbox="allow-scripts"
        srcDoc={doc.html}
      />
      <span className="runtime-constitution-hash" data-hash={doc.blake2_256} />
    </section>
  );
}
