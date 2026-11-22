// 按规范十六进制账户标识比较管理员身份。

export function sameHexAccount(a: string | null | undefined, b: string | null | undefined): boolean {
  if (!a || !b) return false;
  return a.trim().replace(/^0x/i, '').toLowerCase() === b.trim().replace(/^0x/i, '').toLowerCase();
}
