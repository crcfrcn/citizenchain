// CID 前端唯一的用户提示入口。
// 所有业务页面都必须通过本文件显示提示,避免英文原始错误、重复提示和连续弹窗。

import { message, Modal } from 'antd';
import type { ArgsProps } from 'antd/es/message';
import type { ModalFuncProps } from 'antd';
import { ApiError, AuthExpiredError } from './http';

const NOTICE_KEY = 'cid-global-notice';
const DUPLICATE_WINDOW_MS = 1200;

let lastNotice: { type: ArgsProps['type']; content: string; at: number } | null = null;

message.config({
  maxCount: 1,
  duration: 2.4,
});

export const notice = {
  success(content: unknown, fallback = '操作成功') {
    show('success', normalizeNoticeText(content, fallback));
  },
  info(content: unknown, fallback = '操作已取消') {
    show('info', normalizeNoticeText(content, fallback));
  },
  warning(content: unknown, fallback = '请检查输入内容') {
    show('warning', normalizeNoticeText(content, fallback));
  },
  error(error: unknown, fallback = '操作失败') {
    const text = normalizeNoticeText(error, fallback);
    if (isCancelText(text)) {
      show('info', text);
      return;
    }
    show('error', text);
  },
  confirm(config: ModalFuncProps) {
    return Modal.confirm({
      okText: '确定',
      cancelText: '取消',
      ...config,
    });
  },
  warningModal(config: ModalFuncProps) {
    return Modal.warning({
      okText: '知道了',
      ...config,
    });
  },
  destroy() {
    message.destroy();
  },
};

export function normalizeNoticeText(input: unknown, fallback = '操作失败'): string {
  const raw = extractMessage(input, fallback).trim();
  if (!raw) return fallback;
  return translateKnownMessage(raw, fallback);
}

function show(type: ArgsProps['type'], content: string) {
  const now = Date.now();
  if (
    lastNotice &&
    lastNotice.type === type &&
    lastNotice.content === content &&
    now - lastNotice.at < DUPLICATE_WINDOW_MS
  ) {
    return;
  }
  lastNotice = { type, content, at: now };
  message.destroy();
  message.open({ key: NOTICE_KEY, type, content });
}

function extractMessage(input: unknown, fallback: string): string {
  if (typeof input === 'string') return input;
  if (input instanceof AuthExpiredError) return '登录已过期，请重新登录';
  if (input instanceof ApiError) return apiErrorText(input, fallback);
  if (input instanceof DOMException) return domExceptionText(input, fallback);
  if (input instanceof Error) return input.message || fallback;
  if (input && typeof input === 'object' && 'message' in input) {
    const messageValue = (input as { message?: unknown }).message;
    if (typeof messageValue === 'string') return messageValue;
  }
  return fallback;
}

function apiErrorText(error: ApiError, fallback: string): string {
  if (error.status === 401) return '登录已过期，请重新登录';
  // 403 既可能是通用权限不足,也可能是精确的跨省/跨市业务原因。
  // 后端已返回中文业务原因时优先展示,避免统一映射盖住真实问题。
  if (error.status === 403 && error.message && !isGenericForbiddenText(error.message)) {
    return translateKnownMessage(error.message, fallback);
  }
  if (error.errorCode) {
    const mapped = translateErrorCode(error.errorCode);
    if (mapped) return mapped;
  }
  return error.message || fallback;
}

function isGenericForbiddenText(message: string): boolean {
  const lower = message.trim().toLowerCase();
  return !lower || lower === 'forbidden' || lower === 'permission denied';
}

function domExceptionText(error: DOMException, fallback: string): string {
  if (error.name === 'NotAllowedError') return '浏览器拒绝该操作或操作已取消';
  if (error.name === 'AbortError') return '操作已取消';
  if (error.name === 'SecurityError') return '当前页面不允许该操作';
  if (error.name === 'NotSupportedError') return '当前浏览器不支持该操作';
  return error.message || fallback;
}

function translateErrorCode(code: string): string | null {
  const map: Record<string, string> = {
    ONCHINA_ACCOUNT_ID_EXISTS_AS_FEDERAL_REGISTRY_ADMIN: '该账户已是联邦注册局管理员，不能作为新账户使用',
    ONCHINA_ACCOUNT_ID_EXISTS_AS_CITY_REGISTRY_ADMIN: '该账户已是市注册局管理员，不能作为新账户使用',
    ONCHINA_ADMIN_REPLACEMENT_NOT_ONCHAIN: '新账户还不是链上有效管理员，不能完成更换',
    ONCHINA_ADMIN_CITY_REGISTRY_CITY_LIMIT_REACHED: '本市市注册局管理员已满 30 人，不能继续新增',
    ONCHINA_ADMIN_SECURITY_GRANT_REQUIRED: '请先完成扫码签名确认',
    ONCHINA_AUTH_FORBIDDEN: '权限不足，无法执行该操作',
    ONCHINA_AUTH_UNAUTHORIZED: '请先登录管理员账户',
    ONCHINA_REQUEST_INVALID: '请求内容不正确，请检查后重试',
    ONCHINA_RESOURCE_NOT_FOUND: '数据不存在',
    ONCHINA_RESOURCE_CONFLICT: '数据状态冲突，请刷新后重试',
    ONCHINA_RESOURCE_EXPIRED: '请求已过期，请重新操作',
    ONCHINA_RATE_LIMITED: '请求过于频繁，请稍后重试',
    ONCHINA_SERVICE_UNAVAILABLE: '服务暂不可用，请稍后重试',
    ONCHINA_TLS_CA_UNAVAILABLE: '机构 CA 证书暂不可用，请确认链上中国平台已正常启动',
    ONCHINA_LOGIN_CAMERA_UNSUPPORTED: '当前浏览器不支持摄像头扫码，请更换新版浏览器',
    ONCHINA_LOGIN_CAMERA_INSECURE_CONTEXT: '当前页面不是 HTTPS 安全环境，无法使用摄像头',
    ONCHINA_LOGIN_CAMERA_PERMISSION_DENIED: '摄像头权限被拒绝，请在浏览器中允许摄像头权限',
    ONCHINA_LOGIN_CAMERA_OPEN_FAILED: '无法打开摄像头，请检查摄像头权限或设备占用',
    ONCHINA_LOGIN_QR_PARSE_FAILED: '签名二维码解析失败，请重新扫码',
    ONCHINA_LOGIN_QR_NOT_RESPONSE: '扫到的不是登录签名响应二维码',
    ONCHINA_LOGIN_QR_MISSING_FIELD: '签名二维码缺少必要字段，请重新扫码',
    ONCHINA_LOGIN_QR_BAD_PROTO: '二维码协议不正确，请使用新版公民钱包扫码',
    ONCHINA_LOGIN_DESKTOP_GOVERNANCE_UNSUPPORTED: '国家储委会、省储委会、省储行使用节点桌面端管理，不支持登录链上中国平台',
    ONCHINA_LOGIN_PERSONAL_MULTISIG_UNSUPPORTED: '个人多签账户不支持登录链上中国平台',
    ONCHINA_LOGIN_QR_BAD_KIND: '二维码类型不正确，请扫描公民钱包生成的签名响应',
    ONCHINA_LOGIN_QR_BAD_PUBKEY: '签名账户格式无效',
    ONCHINA_LOGIN_QR_BAD_SIGNATURE: '签名格式无效',
    ONCHINA_LOGIN_WALLET_CODE_INVALID:
      '管理员账户码无效，请在公民钱包「账户详情」右上角二维码重新扫描',
    ONCHINA_LOGIN_ORIGIN_REQUIRED: '登录来源缺失，请刷新页面后重试',
    ONCHINA_LOGIN_SESSION_REQUIRED: '登录会话缺失，请刷新页面后重试',
    ONCHINA_LOGIN_DOMAIN_REQUIRED: '登录域名缺失，请使用 https://onchina.local:8964 访问',
    ONCHINA_LOGIN_CHALLENGE_CREATE_FAILED: '登录请求保存失败，请稍后重试',
    ONCHINA_LOGIN_REQUEST_INVALID: '登录请求内容不完整，请重新扫码',
    ONCHINA_LOGIN_RESULT_PARAM_REQUIRED: '登录轮询参数缺失，请刷新页面后重试',
    ONCHINA_LOGIN_CHALLENGE_NOT_FOUND: '登录二维码不存在或已失效，请重新扫描管理员账户码',
    ONCHINA_LOGIN_CHALLENGE_CONSUMED: '登录二维码已使用，请重新扫描管理员账户码',
    ONCHINA_LOGIN_SESSION_MISMATCH: '登录会话不匹配，请关闭多余页面后重新扫描管理员账户码',
    ONCHINA_LOGIN_CHALLENGE_EXPIRED: '登录二维码已过期，请重新扫描管理员账户码',
    ONCHINA_LOGIN_SIGNER_MISMATCH: '签名账户和登录目标账户不一致',
    ONCHINA_LOGIN_SIGNATURE_VERIFY_FAILED: '签名验签失败，请重新扫码签名',
    ONCHINA_LOGIN_COMPLETE_FAILED: '登录签名响应处理失败，请查看服务日志',
    ONCHINA_LOGIN_RESULT_SAVE_FAILED: '登录结果保存失败，请稍后重试',
    ONCHINA_LOGIN_RESULT_QUERY_FAILED: '查询登录结果失败，请稍后重试',
    ONCHINA_LOGIN_ADMIN_NOT_ONCHAIN: '当前钱包不是本机构链上有效管理员',
    ONCHINA_LOGIN_CHAIN_UNREACHABLE: '无法连接区块链节点，请确认节点已启动并同步',
    ONCHINA_LOGIN_NODE_BINDING_REQUIRED: '请先确认本节点绑定机构',
    ONCHINA_LOGIN_NODE_BINDING_MISSING: '本节点尚未绑定机构，请重新扫码登录并确认绑定',
    ONCHINA_LOGIN_NODE_BINDING_INVALID: '节点机构绑定状态异常，无法登录',
    ONCHINA_LOGIN_NODE_BINDING_QUERY_FAILED: '节点机构绑定状态查询失败，请稍后重试',
    ONCHINA_LOGIN_NODE_BINDING_ALREADY_INACTIVE: '节点机构绑定已解除，请重新扫码登录',
    ONCHINA_LOGIN_NODE_BINDING_CHALLENGE_NOT_FOUND: '节点机构绑定请求不存在，请重新扫码登录',
    ONCHINA_LOGIN_NODE_BINDING_CHALLENGE_CONSUMED: '节点机构绑定请求已使用，请重新扫码登录',
    ONCHINA_LOGIN_NODE_BINDING_CHALLENGE_EXPIRED: '节点机构绑定请求已过期，请重新扫码登录',
    ONCHINA_LOGIN_NODE_BINDING_REQUEST_INVALID: '节点机构绑定请求不完整，请重新扫码登录',
    ONCHINA_LOGIN_NODE_BINDING_CANDIDATE_NOT_FOUND: '所选机构不在本次登录候选中，请重新扫码登录',
    ONCHINA_LOGIN_NODE_BINDING_ADMIN_MISMATCH: '当前管理员已不属于所选机构，无法绑定本节点',
    ONCHINA_LOGIN_STATE_ERROR: '登录状态读写失败，请稍后重试',
    ONCHINA_LOGIN_SCOPE_UNAVAILABLE:
      '无法确定该管理员的行政作用域，登录被拒绝。重试无效，请核对该机构的 CID 与省市编码',
    ONCHINA_BIND_SIGNATURE_VERIFY_FAILED: '签名验签失败，请重新扫码签名',
    ONCHINA_REGISTRY_MAIN_ACCOUNT_MISSING: '当前注册局缺少机构主账户绑定，请重新登录绑定本节点或检查链上投影',
  };
  return map[code] ?? null;
}

function translateKnownMessage(raw: string, fallback: string): string {
  const text = raw.trim();
  const lower = text.toLowerCase();
  const exact = KNOWN_ENGLISH_MESSAGES[lower];
  if (exact) return exact;

  if (lower.includes('aborterror')) return '操作已取消';
  if (lower.includes('networkerror') || lower.includes('failed to fetch')) {
    return '无法连接服务器，请检查网络或服务状态';
  }
  if (lower.startsWith('request failed')) return fallback;
  if (lower.endsWith(' is required')) return requiredFieldText(lower);
  if (lower.startsWith('invalid ')) return invalidFieldText(lower);
  if (lower.startsWith('unknown ')) return unknownFieldText(lower);
  if (lower.includes(' query failed')) return '查询失败，请稍后重试';
  if (lower.includes(' create failed')) return '创建失败，请稍后重试';
  if (lower.includes(' update failed')) return '更新失败，请稍后重试';
  if (lower.includes(' delete failed')) return '删除失败，请稍后重试';
  if (lower.includes('write file failed')) return '文件写入失败';
  if (lower.includes('create dir failed')) return '目录创建失败';
  if (lower.includes('file not found')) return '文件不存在';

  // 最后兜底。用户可见提示不允许裸露后端英文原文。
  if (isPlainEnglishLike(text)) return fallback || '操作失败';

  return text || fallback;
}

function isCancelText(text: string): boolean {
  return text.includes('已取消') || text.includes('操作已取消');
}

const KNOWN_ENGLISH_MESSAGES: Record<string, string> = {
  'security grant required': '请先完成扫码签名确认',
  'citizen wallet confirmation required first': '请先完成公民钱包确认',
  'admin auth required': '请先登录管理员账户',
  'federal admin required': '需要联邦注册局管理员权限',
  'initial federal admin required': '需要初始联邦注册局管理员权限',
  'province scope required': '缺少省级权限范围',
  'admin province scope missing': '管理员省级权限范围缺失，无法登录',
  'admin city scope missing': '缺少管理员市级权限范围',
  'city out of scope': '城市超出当前管理员权限范围',
  'out of admin scope': '超出当前管理员权限范围',
  'rate limit exceeded': '请求过于频繁，请稍后重试',
  'sign request not found': '登录二维码不存在或已失效，请重新扫描管理员账户码',
  'sign request already consumed': '登录二维码已使用，请重新扫描管理员账户码',
  'sign request session mismatch': '登录会话不匹配，请关闭多余页面后重新扫描管理员账户码',
  'sign request expired': '登录二维码已过期，请重新扫描管理员账户码',
  'account_id must match targeted account_id': '签名账户和登录目标账户不一致',
  'signature verify failed': '签名验签失败，请重新扫码签名',
  'login signature verify failed': '签名验签失败，请重新扫码签名',
  'not an on-chain admin': '当前钱包不是本机构链上有效管理员',
  'desktop governance institution is not supported by OnChina': '国家储委会、省储委会、省储行使用节点桌面端管理，不支持登录链上中国平台',
  'personal multisig is not supported by OnChina': '个人多签账户不支持登录链上中国平台',
  'chain unreachable': '无法连接区块链节点，请确认节点已启动并同步',
  'node binding required': '请先确认本节点绑定机构',
  'node binding missing': '本节点尚未绑定机构，请重新扫码登录并确认绑定',
  'node binding invalid': '节点机构绑定状态异常，无法登录',
  'node binding query failed': '节点机构绑定状态查询失败，请稍后重试',
  'node binding already inactive': '节点机构绑定已解除，请重新扫码登录',
  'node binding challenge not found': '节点机构绑定请求不存在，请重新扫码登录',
  'node binding challenge already consumed': '节点机构绑定请求已使用，请重新扫码登录',
  'node binding challenge expired': '节点机构绑定请求已过期，请重新扫码登录',
  'binding_challenge_id and candidate_id are required': '节点机构绑定请求不完整，请重新扫码登录',
  'selected institution candidate not found': '所选机构不在本次登录候选中，请重新扫码登录',
  'admin no longer belongs to selected institution': '当前管理员已不属于所选机构，无法绑定本节点',
  'login state error': '登录状态读写失败，请稍后重试',
  'build login qr signature failed': '登录二维码签发失败，请检查节点平台配置',
  'binding not vote eligible': '该公民不具备投票资格',
  'binding not found': '未找到公民档案',
  'citizen not vote eligible': '该公民不具备投票资格',
  'citizen archive not found': '未找到公民档案',
  'citizen record not found': '未找到公民记录',
  'citizen record is not bound': '公民档案不完整',
  'citizen status is stale': '公民状态已过期，请重新同步',
  'account_id already bound': '该钱包账户已绑定其他 CID',
  'challenge expired': '签名请求已过期，请重新操作',
  'invalid signature hex': '签名格式无效',
  'institution not found': '机构不存在',
  'account not found': '账户不存在',
  'document not found': '资料不存在',
  'invalid target status': '目标状态无效',
  'field too long': '字段长度超出限制',
  'reason too long': '原因长度超出限制',
  'file is empty': '文件为空',
  'invalid doc_type': '资料类型无效',
  'invalid page cursor': '分页游标无效',
};

const FIELD_LABELS: Record<string, string> = {
  address: '地址',
  account_id: '账户 ID',
  ss58_address: 'SS58 地址',
  identity_code: '身份CID',
  name: '名称',
  province: '省份',
  city: '城市',
  institution: '机构',
  cid_full_name: '机构全称',
  cid_number: '身份CID',
  file: '文件',
  domain: '域名',
  origin: '来源',
  session_id: '会话',
  identity_qr: '身份二维码',
  signature: '签名',
  payload_hash: '载荷哈希',
  bind_nonce: '绑定随机数',
  binding_seed: '绑定种子',
};

function requiredFieldText(lower: string): string {
  const field = lower.replace(/ is required$/, '').trim();
  const label = FIELD_LABELS[field];
  return label ? `请填写${label}` : '请填写必填项';
}

function invalidFieldText(lower: string): string {
  const field = lower.replace(/^invalid /, '').trim();
  const label = FIELD_LABELS[field];
  return label ? `${label}无效` : '输入内容无效';
}

function unknownFieldText(lower: string): string {
  const field = lower.replace(/^unknown /, '').trim();
  const label = FIELD_LABELS[field];
  return label ? `${label}不存在` : '数据不存在';
}

function isPlainEnglishLike(text: string): boolean {
  return /[A-Za-z]/.test(text) && !/[\u4e00-\u9fff]/.test(text);
}
