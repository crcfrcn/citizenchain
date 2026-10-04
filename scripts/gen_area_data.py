#!/usr/bin/env python3
"""从 china.sqlite 生成 area_data.bin(创世直铸行政区常量真源,ADR-031 卡3)。

china.sqlite 是行政区唯一真源(ADR-021);本脚本把其省/市/镇快照编成紧凑二进制,
供 primitives no_std 端 include_bytes! 读取,创世直铸「行政区 × 机构码模板」派生
全部市行政区/镇行政区公权机构。幂等:重跑覆盖 area_data.bin。

格式(小端 u16):
  u16 省数
  每省: [2]省码(ascii) u8 名长 名(utf-8); u16 市数
    每市: [3]市码 u8 名长 名; u16 镇数
      每镇: [3]镇码 u8 名长 名
"""
import pathlib
import sqlite3
import struct

# china.sqlite 不复制进 primitives(73MB);直接读 onchina 侧真源。
DB = pathlib.Path(__file__).resolve().parent.parent / "onchina/src/cid/china/china.sqlite"
OUT = pathlib.Path(__file__).resolve().parent.parent / "runtime/primitives/cid/china/area_data.bin"


def s(text: str) -> bytes:
    b = text.encode("utf-8")
    assert len(b) < 256, text
    return bytes([len(b)]) + b


def main() -> None:
    # 输入始终只读；路径失效时直接失败，不能创建新的空数据库。
    con = sqlite3.connect(DB.as_uri() + "?mode=ro", uri=True)
    con.row_factory = sqlite3.Row
    provs = con.execute("SELECT code, name FROM provinces ORDER BY sort_order, code").fetchall()
    buf = bytearray()
    buf += struct.pack("<H", len(provs))
    n_city = n_town = 0
    for p in provs:
        pc = p["code"]
        assert len(pc) == 2, pc
        buf += pc.encode("ascii") + s(p["name"])
        cities = con.execute(
            "SELECT code, name FROM cities WHERE province_code=? ORDER BY sort_order, code",
            (pc,),
        ).fetchall()
        buf += struct.pack("<H", len(cities))
        for c in cities:
            cc = c["code"]
            assert len(cc) == 3, cc
            n_city += 1
            buf += cc.encode("ascii") + s(c["name"])
            towns = con.execute(
                "SELECT code, name FROM towns WHERE province_code=? AND city_code=? "
                "ORDER BY sort_order, code",
                (pc, cc),
            ).fetchall()
            buf += struct.pack("<H", len(towns))
            for t in towns:
                tc = t["code"]
                assert len(tc) == 3, tc
                n_town += 1
                buf += tc.encode("ascii") + s(t["name"])
    OUT.write_bytes(buf)
    print(f"provinces={len(provs)} cities={n_city} towns={n_town} bytes={len(buf)} -> {OUT.name}")


if __name__ == "__main__":
    main()
