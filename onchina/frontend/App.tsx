// 中文文件头:App.tsx = 登录态壳子 + Layout 壳子。
// 本文件只负责:
//   1. 挂 <AuthProvider>(AppOuter)
//   2. 渲染 Layout / Header / Content(AppInner)
//   3. 刷新登录态并把机构工作台交给 workspace/WorkspaceRouter
//
// 新增机构 UI 一律放到 frontend/workspace 或对应业务目录,不得继续把机构差异塞回 App.tsx。
import { useEffect, useState } from 'react';
import { QrcodeOutlined } from '@ant-design/icons';
import { Button, Card, Layout, Typography } from 'antd';
import { AuthProvider } from './auth/AuthContext';
import { useAuth } from './hooks/useAuth';
import { writeStoredAuth, clearStoredAuth } from './utils/storedAuth';
import type { AdminAuth } from './auth/types';
import { adminLogout, checkAdminAuth } from './auth/api';
import { getPasskeyStatus } from './auth/passkey/passkeyClient';
import type { CidMetaResult } from './china/api';
import { LoginView, OrganizationCaNotice } from './auth/LoginView';
import { WorkspaceRouter } from './workspace/WorkspaceRouter';
import { notice } from './utils/notice';

const { Header, Content } = Layout;

/** Header 右上角管理员身份；姓名仍保留姓、名两个精确字段，渲染时才拼接。 */
function resolveHeaderAdminIdentity(auth: AdminAuth | null): { cidShortName: string; family_name: string; given_name: string } {
  if (!auth) return { cidShortName: '', family_name: '', given_name: '' };
  const cidShortName = typeof auth.cid_short_name === 'string' ? auth.cid_short_name.trim() : '';
  return { cidShortName, family_name: auth.family_name.trim(), given_name: auth.given_name.trim() };
}

function AppInner() {
  const { auth, setAuth, capabilities } = useAuth();
  const [bootstrapping, setBootstrapping] = useState(true);
  const [cidMeta, setCidMeta] = useState<CidMetaResult | null>(null);
  // 当前管理员是否已注册 passkey(null=未知);工作台据此决定注册局默认落点。
  const [passkeyRegistered, setPasskeyRegistered] = useState<boolean | null>(null);

  useEffect(() => {
    setCidMeta(null);
  }, [auth?.account_id, auth?.institution_code]);

  useEffect(() => {
    let cancelled = false;
    const bootstrap = async () => {
      if (!auth) {
        setBootstrapping(false);
        return;
      }
      try {
        const checked = await checkAdminAuth(auth);
        const refreshedAuth: AdminAuth = {
          ...auth,
          account_id: checked.account_id,
          institution_cid_number: checked.institution_cid_number,
          institution_code: checked.institution_code,
          admin_level: checked.admin_level ?? null,
          capabilities: checked.capabilities,
          workspace: checked.workspace ?? auth.workspace,
          family_name: checked.family_name,
          given_name: checked.given_name,
          scope_province_name: checked.scope_province_name ?? null,
          scope_city_name: checked.scope_city_name ?? null,
          scope_town_name: checked.scope_town_name ?? null,
          cid_short_name: checked.cid_short_name ?? null,
        };
        setAuth(refreshedAuth);
        writeStoredAuth(refreshedAuth);
      } catch {
        if (!cancelled) {
          clearStoredAuth();
          setAuth(null);
        }
      } finally {
        if (!cancelled) setBootstrapping(false);
      }
    };
    bootstrap();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!auth) {
      setPasskeyRegistered(null);
      return;
    }
    let cancelled = false;
    setPasskeyRegistered(null);
    getPasskeyStatus(auth)
      // 出错保守视为已注册,避免误锁定工作台入口。
      .then((registered) => {
        if (!cancelled) setPasskeyRegistered(registered);
      })
      .catch(() => {
        if (!cancelled) setPasskeyRegistered(true);
      });
    return () => {
      cancelled = true;
    };
  }, [auth?.account_id, auth?.institution_code]);

  const headerAdminIdentity = resolveHeaderAdminIdentity(auth);

  const onLogout = () => {
    // best-effort 通知后端销毁 session,不阻塞前端退出。
    if (auth) adminLogout(auth);
    setAuth(null);
    clearStoredAuth();
    setCidMeta(null);
    setPasskeyRegistered(null);
    notice.success('已退出登录');
  };

  return (
    <Layout
      style={{
        minHeight: '100vh',
        background: 'linear-gradient(145deg, #0f172a 0%, #134e4a 42%, #0f766e 100%)',
        backgroundAttachment: 'fixed',
        position: 'relative',
      }}
    >
      <Header
        style={{
          position: 'relative',
          zIndex: 1,
          background: 'linear-gradient(135deg, rgba(13,148,136,0.7) 0%, rgba(15,118,110,0.8) 100%)',
          backdropFilter: 'blur(12px)',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          paddingInline: 32,
          height: 72,
          borderBottom: '1px solid rgba(255,255,255,0.15)',
          boxShadow: '0 2px 16px rgba(0,0,0,0.12)',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 16 }}>
          <div
            style={{
              width: 44,
              height: 44,
              borderRadius: 10,
              background: 'rgba(255,255,255,0.18)',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              fontSize: 22,
              border: '1px solid rgba(255,255,255,0.25)',
            }}
          >
            <QrcodeOutlined style={{ color: '#fff' }} />
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', lineHeight: 1.2 }}>
            <Typography.Text style={{ color: '#ffffff', fontSize: 20, fontWeight: 700, letterSpacing: 2 }}>
              中华民族联邦共和国
            </Typography.Text>
            <Typography.Text
              style={{
                color: 'rgba(255,255,255,0.8)',
                fontSize: 13,
                fontWeight: 500,
                letterSpacing: 4,
                marginTop: 2,
              }}
            >
              链上中国平台
            </Typography.Text>
          </div>
        </div>
        {auth && (
          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <Typography.Text
              style={{
                color: '#ffffff',
                fontSize: 14,
                fontWeight: 500,
                background: 'rgba(255,255,255,0.12)',
                padding: '6px 16px',
                borderRadius: 8,
                border: '1px solid rgba(255,255,255,0.15)',
                display: 'inline-flex',
                alignItems: 'center',
                gap: 8,
              }}
            >
              {headerAdminIdentity.cidShortName && (
                <>
                  <span>{headerAdminIdentity.cidShortName}</span>
                  <span style={{ display: 'inline-flex', alignItems: 'center', justifyContent: 'center', lineHeight: 1 }}>
                    ·
                  </span>
                </>
              )}
              <span>
                {`${headerAdminIdentity.family_name}${headerAdminIdentity.given_name}` || '暂未设置'}
              </span>
            </Typography.Text>
            <Button
              size="small"
              danger
              onClick={onLogout}
              style={{
                background: 'rgba(255,255,255,0.1)',
                borderColor: 'rgba(255,255,255,0.25)',
                color: '#fca5a5',
                fontWeight: 500,
                borderRadius: 8,
              }}
            >
              退出
            </Button>
          </div>
        )}
      </Header>

      {bootstrapping ? (
        <Content
          style={{
            position: 'relative',
            zIndex: 1,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            padding: 24,
          }}
        >
          <Card bordered={false} style={{ width: 520, maxWidth: '92vw' }} loading />
        </Content>
      ) : !auth ? (
        <Content
          style={{
            position: 'relative',
            zIndex: 1,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            padding: 24,
            minHeight: 'calc(100vh - 72px)',
          }}
        >
          <LoginView />
        </Content>
      ) : (
        <Content style={{ position: 'relative', zIndex: 1, padding: '16px 24px 24px' }}>
          <OrganizationCaNotice compact />
          <WorkspaceRouter
            auth={auth}
            capabilities={capabilities}
            passkeyRegistered={passkeyRegistered}
            cidMeta={cidMeta}
            setCidMeta={setCidMeta}
          />
        </Content>
      )}
    </Layout>
  );
}

/**
 * AppOuter:只负责挂 <AuthProvider>。
 */
export default function App() {
  return (
    <AuthProvider>
      <AppInner />
    </AuthProvider>
  );
}
