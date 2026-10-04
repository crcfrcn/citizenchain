#!/usr/bin/env python3
"""检查公民宪法创世冻结条件。

本脚本支持读取已有 chainspec 的 raw storage,也支持通过 RPC 读取已物化块 0 storage。
检查项:
1. `:code` 存在,可选校验其字节等于指定 CI WASM。
2. `LegislationYuan::Laws[0]` 是宪法、全国 scope、v1 生效、无待生效版。
3. `LegislationYuan::LawVersions[0][1]` 存在，章号全局唯一、同章节号唯一、条号全局唯一，
   且包含全部不可修改条款。
4. `ConstitutionImmutableManifest` 清单与 v1 条文摘要逐字匹配。
5. `LawsByScope[Constitution][0] == [0]`, `NextLawId == 1`。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from dataclasses import dataclass
from pathlib import Path

PALLET = b"LegislationYuan"
CONSTITUTION_LAW_ID = 0
GENESIS_VERSION = 1
TIER_CONSTITUTION = 0
LAW_STATUS_EFFECTIVE = 1
IMMUTABLE_ARTICLES = [1, 2, 3, 17, 19, 24, 34, 42]
CODE_KEY = "0x3a636f6465"


def twox_128(data: bytes) -> bytes:
    # 完整创世校验才需要计算 Substrate storage key；纯 SCALE 自检不得依赖第三方包。
    try:
        import xxhash
    except ModuleNotFoundError as exc:
        raise RuntimeError("完整创世校验需要安装 Python xxhash 包") from exc

    return (
        xxhash.xxh64(data, seed=0).intdigest().to_bytes(8, "little")
        + xxhash.xxh64(data, seed=1).intdigest().to_bytes(8, "little")
    )


def blake2_128(data: bytes) -> bytes:
    return hashlib.blake2b(data, digest_size=16).digest()


def blake2_256(data: bytes) -> bytes:
    return hashlib.blake2b(data, digest_size=32).digest()


def u32(v: int) -> bytes:
    return v.to_bytes(4, "little")


def u64(v: int) -> bytes:
    return v.to_bytes(8, "little")


def map_prefix(storage: bytes) -> bytes:
    return twox_128(PALLET) + twox_128(storage)


def blake2_128_concat(encoded: bytes) -> bytes:
    return blake2_128(encoded) + encoded


def storage_value(storage: bytes) -> str:
    return "0x" + map_prefix(storage).hex()


def storage_map(storage: bytes, key: bytes) -> str:
    return "0x" + (map_prefix(storage) + blake2_128_concat(key)).hex()


def storage_double_map(storage: bytes, key1: bytes, key2: bytes) -> str:
    return "0x" + (map_prefix(storage) + blake2_128_concat(key1) + blake2_128_concat(key2)).hex()


class Scale:
    def __init__(self, data: bytes) -> None:
        self.data = data
        self.i = 0

    def _need(self, n: int) -> None:
        if self.i + n > len(self.data):
            raise ValueError("SCALE 数据长度不足")

    def u8(self) -> int:
        self._need(1)
        v = self.data[self.i]
        self.i += 1
        return v

    def u32(self) -> int:
        self._need(4)
        v = int.from_bytes(self.data[self.i : self.i + 4], "little")
        self.i += 4
        return v

    def u64(self) -> int:
        self._need(8)
        v = int.from_bytes(self.data[self.i : self.i + 8], "little")
        self.i += 8
        return v

    def raw(self, n: int) -> bytes:
        self._need(n)
        v = self.data[self.i : self.i + n]
        self.i += n
        return v

    def compact(self) -> int:
        first = self.u8()
        mode = first & 0x03
        if mode == 0:
            return first >> 2
        if mode == 1:
            second = self.u8()
            return ((second << 8) | first) >> 2
        if mode == 2:
            rest = self.raw(3)
            return int.from_bytes(bytes([first]) + rest, "little") >> 2
        length = (first >> 2) + 4
        return int.from_bytes(self.raw(length), "little")

    def vec_bytes(self) -> bytes:
        return self.raw(self.compact())

    def opt_bytes(self) -> bytes | None:
        tag = self.u8()
        if tag == 0:
            return None
        if tag != 1:
            raise ValueError(f"非法 Option tag: {tag}")
        return self.vec_bytes()

    def opt_u32(self) -> int | None:
        tag = self.u8()
        if tag == 0:
            return None
        if tag != 1:
            raise ValueError(f"非法 Option tag: {tag}")
        return self.u32()


@dataclass
class Law:
    law_id: int
    tier: int
    scope_code: int
    effective_version: int | None
    latest_version: int
    pending_version: int | None
    status: int


@dataclass
class Version:
    law_id: int
    version: int
    articles: dict[int, bytes]
    published_at: int
    effective_at: int


def parse_law(raw: bytes) -> Law:
    s = Scale(raw)
    law_id = s.u64()
    tier = s.u8()
    scope_code = s.u32()
    houses_len = s.compact()
    # houses = Vec<CidNumber>，每个 CID 自带 SCALE compact 长度；CID 长度不是协议常量，
    # 禁止按历史固定字节数跳过，否则机构 CID 格式调整后会错位解码后续版本字段。
    for _ in range(houses_len):
        s.vec_bytes()
    effective_version = s.opt_u32()
    latest_version = s.u32()
    pending_version = s.opt_u32()
    status = s.u8()
    if s.i != len(raw):
        raise ValueError("Law SCALE 存在未识别尾部字段")
    return Law(law_id, tier, scope_code, effective_version, latest_version, pending_version, status)


def self_test() -> None:
    """锁定 SCALE 解码与宪法章号、同章节号、条号三层唯一性。"""

    def compact_small(value: int) -> bytes:
        if not 0 <= value < 64:
            raise ValueError("self-test 只编码单字节 compact")
        return bytes([value << 2])

    houses = (b"CID", b"ZS000-NRC0A-000000001-2026")
    raw = b"".join(
        (
            u64(0),
            bytes([TIER_CONSTITUTION]),
            u32(0),
            compact_small(len(houses)),
            *(compact_small(len(house)) + house for house in houses),
            bytes([1]),
            u32(GENESIS_VERSION),
            u32(GENESIS_VERSION),
            bytes([0]),
            bytes([LAW_STATUS_EFFECTIVE]),
        )
    )
    law = parse_law(raw)
    if law != Law(0, TIER_CONSTITUTION, 0, 1, 1, None, LAW_STATUS_EFFECTIVE):
        raise AssertionError(f"Law SCALE self-test 失败:{law}")

    def vec_bytes(value: bytes) -> bytes:
        return compact_small(len(value)) + value

    def encode_article(number: int) -> bytes:
        return b"".join(
            (
                u32(number),
                vec_bytes(f"article-{number}".encode()),
                bytes([0]),
                vec_bytes(b"body"),
                bytes([0]),
                compact_small(0),
            )
        )

    def encode_section(number: int, article_numbers: tuple[int, ...]) -> bytes:
        return b"".join(
            (
                u32(number),
                vec_bytes(f"section-{number}".encode()),
                bytes([0]),
                compact_small(len(article_numbers)),
                *(encode_article(article_number) for article_number in article_numbers),
            )
        )

    def encode_chapter(number: int, sections: tuple[tuple[int, tuple[int, ...]], ...]) -> bytes:
        return b"".join(
            (
                u32(number),
                vec_bytes(f"chapter-{number}".encode()),
                bytes([0]),
                compact_small(len(sections)),
                *(encode_section(section_number, article_numbers)
                  for section_number, article_numbers in sections),
            )
        )

    def encode_version(chapters: tuple[tuple[int, tuple[tuple[int, tuple[int, ...]], ...]], ...]) -> bytes:
        return b"".join(
            (
                u64(CONSTITUTION_LAW_ID),
                u32(GENESIS_VERSION),
                vec_bytes(b"constitution"),
                bytes([0]),
                compact_small(len(chapters)),
                *(encode_chapter(chapter_number, sections)
                  for chapter_number, sections in chapters),
                bytes(32),
                bytes([0]),
                u64(0),
                u64(0),
                u64(0),
            )
        )

    # 不同章允许复用相同节号。
    valid_version = encode_version(((1, ((7, (1,)),)), (2, ((7, (2,)),))))
    parsed = parse_version(valid_version)
    if sorted(parsed.articles) != [1, 2]:
        raise AssertionError(f"合法宪法结构解析异常: {sorted(parsed.articles)}")

    invalid_versions = (
        (encode_version(((1, ((1, (1,)),)), (1, ((1, (2,)),)))), "重复章号"),
        (encode_version(((1, ((3, (1,)), (3, (2,)))),)), "重复节号"),
        (encode_version(((1, ((1, (9,)),)), (2, ((1, (9,)),)))), "重复条号"),
    )
    for invalid_version, expected_error in invalid_versions:
        try:
            parse_version(invalid_version)
        except ValueError as exc:
            if expected_error not in str(exc):
                raise AssertionError(f"结构错误类型异常: {exc}") from exc
        else:
            raise AssertionError(f"未拒绝宪法{expected_error}")
    print("constitution SCALE self-test ok")


def skip_clause(s: Scale) -> None:
    s.u32()
    s.vec_bytes()
    s.opt_bytes()


def parse_article(s: Scale) -> tuple[int, bytes]:
    start = s.i
    number = s.u32()
    s.vec_bytes()
    s.opt_bytes()
    s.vec_bytes()
    s.opt_bytes()
    for _ in range(s.compact()):
        skip_clause(s)
    return number, s.data[start : s.i]


def parse_section(s: Scale, articles: dict[int, bytes], section_numbers: set[int]) -> None:
    section_number = s.u32()
    if section_number in section_numbers:
        raise ValueError(f"同一章出现重复节号: {section_number}")
    section_numbers.add(section_number)
    s.vec_bytes()
    s.opt_bytes()
    for _ in range(s.compact()):
        number, raw_article = parse_article(s)
        if number in articles:
            raise ValueError(f"宪法全文出现重复条号: {number}")
        articles[number] = raw_article


def parse_chapter(s: Scale, articles: dict[int, bytes], chapter_numbers: set[int]) -> None:
    chapter_number = s.u32()
    if chapter_number in chapter_numbers:
        raise ValueError(f"宪法全文出现重复章号: {chapter_number}")
    chapter_numbers.add(chapter_number)
    s.vec_bytes()
    s.opt_bytes()
    section_numbers: set[int] = set()
    for _ in range(s.compact()):
        parse_section(s, articles, section_numbers)


def parse_version(raw: bytes) -> Version:
    s = Scale(raw)
    law_id = s.u64()
    version = s.u32()
    s.vec_bytes()
    s.opt_bytes()
    articles: dict[int, bytes] = {}
    chapter_numbers: set[int] = set()
    for _ in range(s.compact()):
        parse_chapter(s, articles, chapter_numbers)
    s.raw(32)
    s.u8()
    s.u64()
    published_at = s.u64()
    effective_at = s.u64()
    return Version(law_id, version, articles, published_at, effective_at)


def parse_vec_u64(raw: bytes) -> list[int]:
    s = Scale(raw)
    return [s.u64() for _ in range(s.compact())]


def parse_manifest(raw: bytes) -> tuple[list[int], list[bytes]]:
    s = Scale(raw)
    numbers = [s.u32() for _ in range(s.compact())]
    hashes = [s.raw(32) for _ in range(s.compact())]
    return numbers, hashes


class RpcTop:
    """--rpc 模式:以 state_getStorage(key, at) 透明替代 raw.top 字典。

    plain chainspec(ADR-031 D5)不再物化 GB 级 raw state,检查改为
    对临时节点的创世块按键查询,键与断言逻辑与文件模式完全一致。
    """

    def __init__(self, url: str, at: str | None) -> None:
        from urllib.parse import urlsplit
        parsed = urlsplit(url)
        if parsed.scheme != "https" or not parsed.hostname or parsed.username or parsed.password or parsed.fragment:
            raise ValueError("RPC 必须配置完整 HTTPS 地址")
        self.url = url
        self.at = at

    def get(self, key: str) -> str | None:
        import urllib.request

        if not key.startswith("0x"):
            key = "0x" + key
        params = [key] + ([self.at] if self.at else [])
        body = json.dumps(
            {"jsonrpc": "2.0", "id": 1, "method": "state_getStorage", "params": params}
        ).encode()
        req = urllib.request.Request(
            self.url, data=body, headers={"content-type": "application/json"}
        )
        # 中文注释：保留系统 CA 验证，禁止重定向将 HTTPS 查询降级。
        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, request, fp, code, message, headers, newurl):
                return None
        opener = urllib.request.build_opener(NoRedirect())
        with opener.open(req, timeout=30) as resp:
            return json.loads(resp.read()).get("result")


def top_value(top, key: str, label: str) -> bytes:
    value = top.get(key.lower()) or top.get(key)
    if value is None:
        raise AssertionError(f"缺少 {label}: {key}")
    raw = value[2:] if value.startswith("0x") else value
    return bytes.fromhex(raw)


def check(path: Path | None, expect_code_file: Path | None, rpc_top=None) -> None:
    if rpc_top is not None:
        top = rpc_top
    else:
        spec = json.loads(path.read_text())
        top = spec.get("genesis", {}).get("raw", {}).get("top", {})
        if not isinstance(top, dict):
            raise AssertionError("chainspec 缺 genesis.raw.top")

    code = top_value(top, CODE_KEY, ":code")
    if not code:
        raise AssertionError(":code 为空")
    if expect_code_file is not None:
        expected = expect_code_file.read_bytes()
        if code != expected:
            raise AssertionError(
                f":code 与 WASM 文件不一致: chainspec={len(code)} bytes, wasm={len(expected)} bytes"
            )

    law = parse_law(top_value(top, storage_map(b"Laws", u64(0)), "Laws[0]"))
    assert law.law_id == CONSTITUTION_LAW_ID, f"Laws[0].law_id 异常: {law.law_id}"
    assert law.tier == TIER_CONSTITUTION, f"Laws[0].tier 不是 Constitution: {law.tier}"
    assert law.scope_code == 0, f"Laws[0].scope_code 不是全国 0: {law.scope_code}"
    assert law.effective_version == GENESIS_VERSION, f"宪法创世生效版本应为 v1: {law}"
    assert law.latest_version == GENESIS_VERSION, f"宪法创世最新版本应为 v1: {law}"
    assert law.pending_version is None, f"宪法创世不得有待生效版本: {law}"
    assert law.status == LAW_STATUS_EFFECTIVE, f"宪法创世状态应为 Effective: {law.status}"

    version = parse_version(
        top_value(top, storage_double_map(b"LawVersions", u64(0), u32(1)), "LawVersions[0][1]")
    )
    assert version.law_id == CONSTITUTION_LAW_ID, f"LawVersion law_id 异常: {version.law_id}"
    assert version.version == GENESIS_VERSION, f"LawVersion version 异常: {version.version}"

    missing = [n for n in IMMUTABLE_ARTICLES if n not in version.articles]
    if missing:
        raise AssertionError(f"宪法 v1 缺不可修改条款: {missing}")

    numbers, hashes = parse_manifest(
        top_value(top, storage_value(b"ConstitutionImmutableManifest"), "ConstitutionImmutableManifest")
    )
    assert numbers == IMMUTABLE_ARTICLES, f"manifest 清单异常: {numbers}"
    assert len(hashes) == len(numbers), "manifest 条号与摘要数量不一致"
    for number, digest in zip(numbers, hashes):
        actual = blake2_256(version.articles[number])
        if actual != digest:
            raise AssertionError(f"manifest 第 {number} 条摘要与宪法 v1 条文不一致")

    scope = parse_vec_u64(
        top_value(
            top,
            storage_double_map(b"LawsByScope", bytes([TIER_CONSTITUTION]), u32(0)),
            "LawsByScope[Constitution][0]",
        )
    )
    assert scope == [CONSTITUTION_LAW_ID], f"宪法层级唯一性异常: {scope}"

    next_law_id = Scale(top_value(top, storage_value(b"NextLawId"), "NextLawId")).u64()
    assert next_law_id == 1, f"NextLawId 应为 1: {next_law_id}"

    print("constitution genesis check ok")
    print(f"  spec: {path}")
    print(f"  :code bytes: {len(code)}")
    print("  law_id=0 tier=Constitution effective_version=1 latest_version=1 pending=None")
    print("  immutable articles:", ",".join(str(n) for n in numbers))


def main() -> int:
    parser = argparse.ArgumentParser(description="检查公民宪法创世冻结条件(chainspec 文件或 --rpc 临时节点)")
    parser.add_argument("chainspec", type=Path, nargs="?")
    parser.add_argument("--expect-code-file", type=Path)
    parser.add_argument("--rpc", help="临时节点 RPC 地址,必须为可信 HTTPS")
    parser.add_argument("--at", help="创世块哈希(--rpc 模式钉块查询)")
    parser.add_argument("--self-test", action="store_true", help="只运行 SCALE 解码自检")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        return 0

    if args.rpc is None and args.chainspec is None:
        parser.error("必须提供 chainspec 文件或 --rpc")

    try:
        check(
            args.chainspec,
            args.expect_code_file,
            rpc_top=RpcTop(args.rpc, args.at) if args.rpc else None,
        )
    except Exception as exc:  # noqa: BLE001
        print(f"constitution genesis check failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
