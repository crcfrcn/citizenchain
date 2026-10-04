#!/usr/bin/env python3
"""
统一派生 citizenchain/runtime/primitives/cid/china 下所有链上保留地址。

统一方案（GMB 单域 + op_tag 子命名空间）：

    preimage = b"GMB"         (3B)
             || op_tag        (1B)
             || ss58          (2B, LE, = [0xEB, 0x07] for 2027)
             || payload       (按 op_tag 规范拼接)
    address  = blake2b_256(preimage)

op_tag 分配（唯一真源 = account_derive.rs，本脚本运行时从该文件解析，不在此硬编码）：
    0x00 = OP_NAME      → input: cid_number + name_utf8（机构自定义命名账户，永久冻结）
    0x01 = OP_MAIN      → input: cid_number          （机构主账户）
    0x02 = OP_FEE       → input: cid_number          （费用账户）
    0x03 = OP_STAKE     → input: cid_number          （质押账户）
    0x04 = OP_SAFETY    → input: cid_number          （国储会安全基金，仅 NRC）
    0x05 = OP_HE        → input: cid_number          （国储会两和基金，仅 NRC）
    0x06 = OP_PERSONAL  → input: creator(32B) + name_utf8（个人多签，链上派生）
    0x07 = OP_CLEARING  → input: cid_number          （清算账户，清算行资格机构）
    0x08 = OP_FCSF      → input: cid_number          （联邦公民安全基金，仅 FSC）

字节空间分区：0x00-0x0F 地址派生；0x10-0x1D 签名域（sign.rs）；0x1E-0xFF 未分配保留。
新增协议账户只取"当前最大 + 1"，不改动既有 tag（改 tag 会重派生全部地址）。

本工具一次性重算：
  - main_account（所有 7 个机构常量文件：cb/ch/zf/jc/lf/sf/jy）
  - fee_account （cb + ch 共 87 个）
  - stake_account（ch 专有 43 个，按 cid_number 派生）
  - SAFETY_FUND_ACCOUNT（cb 内 1 个全局常量）
  - CHINA_RESERVED_MAIN_ACCOUNTS 保留名单（zb.rs 汇总表，365 条）

用法：
  python3 citizenchain/scripts/rederive_accounts.py               # dry-run，仅打印差异
  python3 citizenchain/scripts/rederive_accounts.py --apply       # 写回源码
"""

import argparse
import hashlib
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional


PRIMITIVES_DIR = Path(__file__).resolve().parent.parent / "runtime" / "primitives"
CHINA_DIR = PRIMITIVES_DIR / "cid" / "china"
CORE_CONST_PATH = PRIMITIVES_DIR / "src" / "core_const.rs"
ACCOUNT_DERIVE_PATH = PRIMITIVES_DIR / "src" / "account_derive.rs"


def _load_core_const() -> tuple[bytes, int, dict[str, int]]:
    """读取派生协议唯一真源：

    - 域分隔符 `GMB` + `SS58_FORMAT` 在 `core_const.rs`（签名亦共用）。
    - 账户 op_tag `OP_MAIN..OP_HE` 在 `account_derive.rs`（单源）。
    """
    core_text = CORE_CONST_PATH.read_text(encoding="utf-8")
    domain_match = re.search(r'pub const GMB:\s*&\[u8;\s*3\]\s*=\s*b"([^"]+)";', core_text)
    ss58_match = re.search(r"pub const SS58_FORMAT:\s*u16\s*=\s*(\d+);", core_text)
    if not domain_match or not ss58_match:
        raise RuntimeError(f"无法从 {CORE_CONST_PATH} 读取 GMB/SS58_FORMAT")

    derive_text = ACCOUNT_DERIVE_PATH.read_text(encoding="utf-8")
    ops: dict[str, int] = {}
    for name in ("OP_MAIN", "OP_FEE", "OP_STAKE", "OP_SAFETY", "OP_HE"):
        match = re.search(rf"pub const {name}:\s*u8\s*=\s*(0x[0-9A-Fa-f]+|\d+);", derive_text)
        if not match:
            raise RuntimeError(f"无法从 {ACCOUNT_DERIVE_PATH} 读取 {name}")
        ops[name] = int(match.group(1), 0)
    return domain_match.group(1).encode("utf-8"), int(ss58_match.group(1)), ops


# ── 统一域 ─────────────────────────────────────────
DOMAIN, SS58_FORMAT, OPS = _load_core_const()
OP_MAIN = OPS["OP_MAIN"]
OP_FEE = OPS["OP_FEE"]
OP_STAKE = OPS["OP_STAKE"]
OP_SAFETY = OPS["OP_SAFETY"]
OP_HE = OPS["OP_HE"]

# 按 china_cb.rs 硬编码，第一条是 NRC
NRC_CID_NUMBER = "LN001-NRC0G-944805165-2026"

# 需要处理的机构文件（含 main_account 字段）
FILES_WITH_MAIN = [
    "china_cb.rs",
    "china_ch.rs",
    "china_sf.rs",
    "china_jc.rs",
    "china_lf.rs",
    "china_jy.rs",
    "china_zf.rs",
    # 公民链技术发展基金会（单条 const，非数组；行级正则同样适用）。
    "citizenchain.rs",
]

# 含 fee_account 字段的机构文件
FILES_WITH_FEE = [
    "china_cb.rs",
    "china_ch.rs",
    "china_zf.rs",
    "china_lf.rs",
    "china_sf.rs",
    "china_jc.rs",
    "china_jy.rs",
    "citizenchain.rs",
]

# 只在 ch 里有 stake_account 和 citizens_number
FILES_WITH_STAKE = ["china_ch.rs"]


# ── 哈希基元 ───────────────────────────────────────
def blake2b_256(data: bytes) -> bytes:
    return hashlib.blake2b(data, digest_size=32).digest()


def ss58_le() -> bytes:
    return SS58_FORMAT.to_bytes(2, byteorder="little")


def derive(op_tag: int, payload: bytes) -> bytes:
    """统一派生入口：GMB + op_tag + ss58 + payload → blake2b_256"""
    preimage = DOMAIN + bytes([op_tag]) + ss58_le() + payload
    return blake2b_256(preimage)


def derive_main(cid_number: str) -> bytes:
    return derive(OP_MAIN, cid_number.encode("utf-8"))


def derive_fee(cid_number: str) -> bytes:
    return derive(OP_FEE, cid_number.encode("utf-8"))


def derive_stake(cid_number: str) -> bytes:
    return derive(OP_STAKE, cid_number.encode("utf-8"))


def derive_safety_fund() -> bytes:
    return derive(OP_SAFETY, NRC_CID_NUMBER.encode("utf-8"))


def derive_he() -> bytes:
    return derive(OP_HE, NRC_CID_NUMBER.encode("utf-8"))


# ── Rust 文件扫描 ───────────────────────────────────
@dataclass
class MainEntry:
    cid_number: str
    old_hex: str
    new_hex: str
    file_name: str
    line_num: int


@dataclass
class FeeEntry:
    cid_number: str
    old_hex: str
    new_hex: str
    file_name: str
    line_num: int


@dataclass
class StakeEntry:
    cid_number: str
    old_hex: str
    new_hex: str
    file_name: str
    line_num: int


def hexstr(b: bytes) -> str:
    return b.hex()


def extract_main(file_path: Path) -> list[MainEntry]:
    """按 cid_number → 下一个 main_account hex!(...) 配对。"""
    text = file_path.read_text(encoding="utf-8")
    lines = text.split("\n")
    out: list[MainEntry] = []

    cid_re = re.compile(r'cid_number:\s*"([^"]+)"')
    addr_re = re.compile(r'main_account:\s*hex!\("([0-9a-fA-F]{64})"\)')

    current_cid: Optional[str] = None
    for i, line in enumerate(lines):
        m1 = cid_re.search(line)
        if m1:
            current_cid = m1.group(1)
        m2 = addr_re.search(line)
        if m2 and current_cid is not None:
            old = m2.group(1).lower()
            new = hexstr(derive_main(current_cid))
            out.append(
                MainEntry(
                    cid_number=current_cid,
                    old_hex=old,
                    new_hex=new,
                    file_name=file_path.name,
                    line_num=i + 1,
                )
            )
            current_cid = None
    return out


def extract_fee(file_path: Path) -> list[FeeEntry]:
    """按 cid_number → 下一个 fee_account hex!(...) 配对。"""
    text = file_path.read_text(encoding="utf-8")
    lines = text.split("\n")
    out: list[FeeEntry] = []

    cid_re = re.compile(r'cid_number:\s*"([^"]+)"')
    addr_re = re.compile(r'fee_account:\s*hex!\("([0-9a-fA-F]{64})"\)')

    current_cid: Optional[str] = None
    for i, line in enumerate(lines):
        m1 = cid_re.search(line)
        if m1:
            current_cid = m1.group(1)
        m2 = addr_re.search(line)
        if m2 and current_cid is not None:
            old = m2.group(1).lower()
            new = hexstr(derive_fee(current_cid))
            out.append(
                FeeEntry(
                    cid_number=current_cid,
                    old_hex=old,
                    new_hex=new,
                    file_name=file_path.name,
                    line_num=i + 1,
                )
            )
    return out


def extract_stake(file_path: Path) -> list[StakeEntry]:
    """按 cid_number → 下一个 stake_account hex!(...) 配对。"""
    text = file_path.read_text(encoding="utf-8")
    lines = text.split("\n")
    out: list[StakeEntry] = []

    cid_re = re.compile(r'cid_number:\s*"([^"]+)"')
    addr_re = re.compile(r'stake_account:\s*hex!\("([0-9a-fA-F]{64})"\)')

    current_cid: Optional[str] = None
    for i, line in enumerate(lines):
        m1 = cid_re.search(line)
        if m1:
            current_cid = m1.group(1)
        m2 = addr_re.search(line)
        if m2 and current_cid is not None:
            old = m2.group(1).lower()
            new = hexstr(derive_stake(current_cid))
            out.append(
                StakeEntry(
                    cid_number=current_cid,
                    old_hex=old,
                    new_hex=new,
                    file_name=file_path.name,
                    line_num=i + 1,
                )
            )
            current_cid = None
    return out


# ── 写回 ────────────────────────────────────────────
def rewrite_main(path: Path, entries: list[MainEntry]) -> None:
    text = path.read_text(encoding="utf-8")
    for e in entries:
        text = text.replace(
            f'main_account: hex!("{e.old_hex}")',
            f'main_account: hex!("{e.new_hex}")',
            1,
        )
    path.write_text(text, encoding="utf-8")


def rewrite_fee(path: Path, entries: list[FeeEntry]) -> None:
    text = path.read_text(encoding="utf-8")
    for e in entries:
        text = text.replace(
            f'fee_account: hex!("{e.old_hex}")',
            f'fee_account: hex!("{e.new_hex}")',
            1,
        )
    path.write_text(text, encoding="utf-8")


def rewrite_stake(path: Path, entries: list[StakeEntry]) -> None:
    text = path.read_text(encoding="utf-8")
    for e in entries:
        text = text.replace(
            f'stake_account: hex!("{e.old_hex}")',
            f'stake_account: hex!("{e.new_hex}")',
            1,
        )
    path.write_text(text, encoding="utf-8")


def rewrite_safety_fund(cb_path: Path, new_hex: str) -> None:
    """重写 china_cb.rs 里 SAFETY_FUND_ACCOUNT 常量。"""
    text = cb_path.read_text(encoding="utf-8")
    # 匹配形如 pub const SAFETY_FUND_ACCOUNT: [u8; 32] = hex!("...")
    pattern = re.compile(
        r'(pub const SAFETY_FUND_ACCOUNT:\s*\[u8;\s*32\]\s*=\s*\n?\s*hex!\(")([0-9a-fA-F]{64})("\))'
    )
    new_text, n = pattern.subn(rf"\g<1>{new_hex}\g<3>", text)
    if n == 0:
        print("⚠️  china_cb.rs 中没找到 SAFETY_FUND_ACCOUNT 常量，跳过")
        return
    cb_path.write_text(new_text, encoding="utf-8")


def rewrite_he(cb_path: Path, new_hex: str) -> None:
    """重写 china_cb.rs 里 NRC_HE_ACCOUNT 常量。"""
    text = cb_path.read_text(encoding="utf-8")
    # 匹配形如 pub const NRC_HE_ACCOUNT: [u8; 32] = hex!("...")
    pattern = re.compile(
        r'(pub const NRC_HE_ACCOUNT:\s*\[u8;\s*32\]\s*=\s*\n?\s*hex!\(")([0-9a-fA-F]{64})("\))'
    )
    new_text, n = pattern.subn(rf"\g<1>{new_hex}\g<3>", text)
    if n == 0:
        print("⚠️  china_cb.rs 中没找到 NRC_HE_ACCOUNT 常量，跳过")
        return
    cb_path.write_text(new_text, encoding="utf-8")


def regen_zb(all_addresses: list[str], dry_run: bool) -> None:
    """重建 china_zb.rs：汇总所有保留地址 main + fee + stake + safety_fund。"""
    zb_path = CHINA_DIR / "china_zb.rs"
    uniq = sorted(set(all_addresses))

    lines = [
        "//! 汇总 runtime/primitives/cid/china 目录下所有制度保留地址",
        "//! （main_account + fee_account + stake_account + SAFETY_FUND_ACCOUNT）。",
        "//! 用于禁止 organization-manage 抢注这些机构地址。",
        "//!",
        "//! 派生统一走 `primitives::core_const::GMB` + op_tag，由",
        "//! `scripts/gmb.py` 一次性生成，禁止手改。",
        "",
        "use hex_literal::hex;",
        "",
        f"pub const CHINA_RESERVED_MAIN_ACCOUNTS: &[[u8; 32]; {len(uniq)}] = &[",
    ]
    for a in uniq:
        lines.append(f'    hex!("{a}"),')
    lines.append("];")
    lines.append("")
    lines.append("/// 检查地址是否属于制度保留地址（静态常量数组二分查找）。")
    lines.append("pub fn is_reserved_main_account(address: &[u8; 32]) -> bool {")
    lines.append("    CHINA_RESERVED_MAIN_ACCOUNTS")
    lines.append("        .binary_search(address)")
    lines.append("        .is_ok()")
    lines.append("}")
    lines.append("")

    new_text = "\n".join(lines)
    if dry_run:
        print(f"\n=== china_zb.rs 将包含 {len(uniq)} 个保留地址 ===")
    else:
        zb_path.write_text(new_text, encoding="utf-8")
        print(f"✅ china_zb.rs 已更新：{len(uniq)} 个保留地址")


# ── 主流程 ──────────────────────────────────────────
def main() -> int:
    parser = argparse.ArgumentParser(
        description="统一派生 primitives/cid/china 下的 main/fee/stake/safety_fund 地址（GMB + op_tag）"
    )
    grp = parser.add_mutually_exclusive_group()
    grp.add_argument("--dry-run", action="store_true", default=True, help="仅打印差异（默认）")
    grp.add_argument("--apply", action="store_true", help="写回源码")
    args = parser.parse_args()
    dry_run = not args.apply

    mode = "🔍 干运行" if dry_run else "✏️  写回"
    print(f"{mode} 模式")
    print(f"Domain: {DOMAIN!r}  SS58: {SS58_FORMAT} ({ss58_le().hex()})\n")

    # 汇总用
    all_reserved: list[str] = []

    # ── main_account ──
    main_total = main_changed = 0
    for fn in FILES_WITH_MAIN:
        fp = CHINA_DIR / fn
        if not fp.exists():
            print(f"⚠️  跳过不存在的 {fn}")
            continue
        entries = extract_main(fp)
        changed = [e for e in entries if e.old_hex != e.new_hex]
        main_total += len(entries)
        main_changed += len(changed)
        print(f"📄 [main]  {fn}: {len(entries)} 条，{len(changed)} 变更")
        for e in entries:
            all_reserved.append(e.new_hex)
            if e.old_hex != e.new_hex:
                print(f"   🔄 {e.cid_number}")
                print(f"      旧: {e.old_hex}")
                print(f"      新: {e.new_hex}")
        if not dry_run and changed:
            rewrite_main(fp, entries)

    # ── fee_account ──
    fee_total = fee_changed = 0
    for fn in FILES_WITH_FEE:
        fp = CHINA_DIR / fn
        if not fp.exists():
            continue
        entries = extract_fee(fp)
        changed = [e for e in entries if e.old_hex != e.new_hex]
        fee_total += len(entries)
        fee_changed += len(changed)
        print(f"📄 [fee]   {fn}: {len(entries)} 条，{len(changed)} 变更")
        for e in entries:
            all_reserved.append(e.new_hex)
            if e.old_hex != e.new_hex:
                print(f"   🔄 {e.cid_number}")
                print(f"      旧: {e.old_hex}")
                print(f"      新: {e.new_hex}")
        if not dry_run and changed:
            rewrite_fee(fp, entries)

    # ── stake_account ──
    stake_total = stake_changed = 0
    for fn in FILES_WITH_STAKE:
        fp = CHINA_DIR / fn
        if not fp.exists():
            continue
        entries = extract_stake(fp)
        changed = [e for e in entries if e.old_hex != e.new_hex]
        stake_total += len(entries)
        stake_changed += len(changed)
        print(f"📄 [stake] {fn}: {len(entries)} 条，{len(changed)} 变更")
        for e in entries:
            all_reserved.append(e.new_hex)
            if e.old_hex != e.new_hex:
                print(f"   🔄 {e.cid_number}")
                print(f"      旧: {e.old_hex}")
                print(f"      新: {e.new_hex}")
        if not dry_run and changed:
            rewrite_stake(fp, entries)

    # ── SAFETY_FUND_ACCOUNT ──
    new_safety_fund = hexstr(derive_safety_fund())
    all_reserved.append(new_safety_fund)
    print(f"\n📄 [safety_fund] SAFETY_FUND_ACCOUNT: {new_safety_fund}")
    if not dry_run:
        rewrite_safety_fund(CHINA_DIR / "china_cb.rs", new_safety_fund)

    # ── NRC_HE_ACCOUNT（两和基金）──
    new_he = hexstr(derive_he())
    all_reserved.append(new_he)
    print(f"📄 [he]     NRC_HE_ACCOUNT:     {new_he}")
    if not dry_run:
        rewrite_he(CHINA_DIR / "china_cb.rs", new_he)

    # ── china_zb.rs 汇总 ──
    regen_zb(all_reserved, dry_run=dry_run)

    print(
        f"\n==== 统计 ====\n"
        f"main : {main_total} 条，{main_changed} 变更\n"
        f"fee  : {fee_total} 条，{fee_changed} 变更\n"
        f"stake: {stake_total} 条，{stake_changed} 变更\n"
        f"safety_fund: 1 条\n"
        f"汇总保留地址: {len(set(all_reserved))} 个唯一\n"
    )
    if dry_run:
        print("💡 --apply 写回源码")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
