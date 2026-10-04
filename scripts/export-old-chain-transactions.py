#!/usr/bin/env python3
"""导出旧链全部交易记录（金额 + 备注），供正式创世清库前留档。

在节点服务器本机执行（RPC 只监听本地，不对公网开放）：

    python3 export-old-chain-transactions.py > old-chain-transactions.json

遍历块 0..best，解析每个区块的 extrinsics：
- OnchainTransaction(4) / transfer_with_remark(0)：收款账户、金额（分）、备注
- 其余 extrinsic 只记 pallet/call 索引与字节长度，保留完整原文 hex 供复核

不依赖 subxt/metadata，纯 JSON-RPC + 手工 SCALE 解码，避免 runtime 升级导致的解析失败。
"""

import json
import os
from urllib.parse import urlsplit
import sys
import urllib.request

RPC = os.environ.get("CITIZENCHAIN_RPC_URL", "")
parsed = urlsplit(RPC)
if parsed.scheme != "https" or not parsed.hostname or parsed.username or parsed.password or parsed.fragment:
    raise ValueError("CITIZENCHAIN_RPC_URL 必须配置完整 HTTPS 地址")

# 中文注释：只允许可信 HTTPS 请求，不跟随服务器重定向，不回退明文。
class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, newurl):
        return None

OPENER = urllib.request.build_opener(NoRedirect())

TRANSFER_PALLET = 4
TRANSFER_CALL = 0


def rpc(method, params=None):
    body = json.dumps(
        {"jsonrpc": "2.0", "id": 1, "method": method, "params": params or []}
    ).encode()
    req = urllib.request.Request(
        RPC, data=body, headers={"Content-Type": "application/json"}
    )
    with OPENER.open(req, timeout=30) as resp:
        out = json.loads(resp.read())
    if "error" in out:
        raise RuntimeError(f"{method} failed: {out['error']}")
    return out["result"]


def compact_u32(data, offset):
    """SCALE compact 整数解码，返回 (值, 新偏移)。"""
    b0 = data[offset]
    mode = b0 & 0b11
    if mode == 0:
        return b0 >> 2, offset + 1
    if mode == 1:
        return int.from_bytes(data[offset : offset + 2], "little") >> 2, offset + 2
    if mode == 2:
        return int.from_bytes(data[offset : offset + 4], "little") >> 2, offset + 4
    n = (b0 >> 2) + 4
    return int.from_bytes(data[offset + 1 : offset + 1 + n], "little"), offset + 1 + n


def parse_transfer(call_body):
    """[beneficiary:32][amount:u128_le][remark:BoundedVec<u8>]"""
    if len(call_body) < 32 + 16 + 1:
        return None
    beneficiary = call_body[:32].hex()
    amount_fen = int.from_bytes(call_body[32:48], "little")
    remark_len, off = compact_u32(call_body, 48)
    remark = call_body[off : off + remark_len].decode("utf-8", errors="replace")
    return {
        "beneficiary_account_id": "0x" + beneficiary,
        "amount_fen": amount_fen,
        "amount_yuan": f"{amount_fen / 100:.2f}",
        "remark": remark,
    }


def main():
    best_hash = rpc("chain_getFinalizedHead")
    best_header = rpc("chain_getHeader", [best_hash])
    best_number = int(best_header["number"], 16)
    genesis_hash = rpc("chain_getBlockHash", [0])

    records = []
    for number in range(best_number + 1):
        block_hash = rpc("chain_getBlockHash", [number])
        if block_hash is None:
            continue
        block = rpc("chain_getBlock", [block_hash])["block"]
        for idx, ext_hex in enumerate(block["extrinsics"]):
            raw = bytes.fromhex(ext_hex[2:])
            # 外层 compact 长度前缀
            _, off = compact_u32(raw, 0)
            body = raw[off:]
            if not body:
                continue
            version = body[0]
            signed = bool(version & 0x80)
            # 已签名 extrinsic 的签名区长度随 TxExtension 变化，无法在不读 metadata 的
            # 情况下精确跳过；这里从尾部反向定位 pallet/call：未签名交易 call 紧跟版本字节。
            entry = {
                "block_number": number,
                "block_hash": block_hash,
                "extrinsic_index": idx,
                "signed": signed,
                "raw_hex": ext_hex,
            }
            if not signed and len(body) >= 3:
                pallet, call = body[1], body[2]
                entry["pallet_index"] = pallet
                entry["call_index"] = call
                if pallet == TRANSFER_PALLET and call == TRANSFER_CALL:
                    parsed = parse_transfer(body[3:])
                    if parsed:
                        entry.update(parsed)
                        entry["kind"] = "transfer_with_remark"
            else:
                # 已签名：在原文中扫描 transfer_with_remark 的 [04][00] 调用头，
                # 命中后按固定布局解析；解析失败则只留原文，由人工复核。
                for p in range(1, len(body) - 49):
                    if body[p] == TRANSFER_PALLET and body[p + 1] == TRANSFER_CALL:
                        parsed = parse_transfer(body[p + 2 :])
                        if parsed and 0 < parsed["amount_fen"] < 10**24:
                            entry.update(parsed)
                            entry["kind"] = "transfer_with_remark"
                            entry["pallet_index"] = TRANSFER_PALLET
                            entry["call_index"] = TRANSFER_CALL
                            break
            records.append(entry)

    json.dump(
        {
            "genesis_hash": genesis_hash,
            "best_finalized_number": best_number,
            "best_finalized_hash": best_hash,
            "total_extrinsics": len(records),
            "records": records,
        },
        sys.stdout,
        ensure_ascii=False,
        indent=2,
    )


if __name__ == "__main__":
    main()
