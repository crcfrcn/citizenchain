import { useEffect, useRef, useState } from 'react';
import { Button, Image, Modal } from 'antd';
import type { AdminAuth } from '../auth/types';
import { adminBlobRequest } from '../utils/http';
import { notice } from '../utils/notice';

export function LegalRepresentativePhoto({ auth, path, name }: {
  auth: AdminAuth;
  path?: string | null;
  name?: string | null;
}) {
  const [src, setSrc] = useState('');
  const [loading, setLoading] = useState(false);
  const pending = useRef<AbortController | null>(null);
  useEffect(() => () => { if (src) URL.revokeObjectURL(src); }, [src]);
  useEffect(() => {
    setSrc('');
    setLoading(false);
    return () => { pending.current?.abort(); };
  }, [path, auth.access_token]);

  if (!path) return <span>(未上传)</span>;
  // 历史磁盘路径无法读取；不把服务端返回的任意地址作为携带令牌的请求目标。
  if (!/^\/api\/institutions\/legal-representative\/photo\/[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(path)) {
    return <span>原照片需重新上传</span>;
  }
  const photoPath = path;
  const show = async () => {
    const controller = new AbortController();
    pending.current?.abort();
    pending.current = controller;
    setLoading(true);
    try {
      const blob = await adminBlobRequest(photoPath, auth, controller.signal);
      if (!controller.signal.aborted) setSrc(URL.createObjectURL(blob));
    } catch (err) {
      if (!controller.signal.aborted) notice.error(err, '证件照读取失败');
    } finally {
      if (!controller.signal.aborted) setLoading(false);
    }
  };
  return <>
    <Button type="link" loading={loading} onClick={() => void show()}>{name || '查看证件照'}</Button>
    <Modal title="法定代表人证件照" open={Boolean(src)} footer={null} onCancel={() => setSrc('')}>
      {src && <Image src={src} alt="法定代表人证件照" preview={false} style={{ maxWidth: '100%' }} />}
    </Modal>
  </>;
}
