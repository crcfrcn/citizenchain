// 机构多签链交互查询 API。机构创建/机构资料维护仍归属
// `frontend/subjects/`;本目录只承载链端 pull CID 信息所需的前端封装。

type ApiEnvelope<T> = {
  code: number;
  message: string;
  data: T;
};

export interface InstitutionInfoDetail {
  cidNumber: string;
  cidFullName?: string | null;
  cidShortName?: string | null;
  category: string;
  subjectProperty: string;
  p1: string;
  provinceName: string;
  cityName: string;
  provinceCode: string;
  cityCode: string;
  institutionCode: string;
  privateType?: string | null;
  partnershipKind?: string | null;
  hasLegalPersonality?: boolean | null;
  parentCidNumber?: string | null;
  legalRepresentative?: {
    familyName: string;
    givenName: string;
    cidNumber: string;
    accountId: string;
  } | null;
}

async function publicAppRequest<T>(path: string): Promise<T> {
  let resp: Response;
  try {
    resp = await fetch(path);
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    throw new Error(`无法连接服务器：${msg}`);
  }

  const text = await resp.text();
  let body: ApiEnvelope<T> | null = null;
  try {
    body = text ? (JSON.parse(text) as ApiEnvelope<T>) : null;
  } catch {
    const snippet = text.slice(0, 120);
    throw new Error(`服务响应格式错误(${resp.status})：${snippet || 'empty body'}`);
  }
  if (!resp.ok || !body || body.code !== 0) {
    throw new Error(body?.message ?? `request failed (${resp.status})`);
  }
  return body.data;
}

export async function getInstitutionInfo(cidNumber: string): Promise<InstitutionInfoDetail> {
  const encoded = encodeURIComponent(cidNumber);
  return publicAppRequest<InstitutionInfoDetail>(`/api/app/institutions/${encoded}`);
}
