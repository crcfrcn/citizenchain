#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
行政区 code 不可变不复用 CI 校验(ADR-021)。

断言:
  1. provinces/cities/towns 表无重复 code —— code 是稳定外键,重复=反查歧义。
  2. 省名、市名全国唯一;省/市/镇不维护墓碑表,只保存当前有效数据。
  3. 镇下完整地址统一使用 addresses 单表,旧 address_units/source_code/raw_name 结构必须清除。
  4. addresses 的 address_name_code 为三位数字,同镇下 code 与 address_name 必须一一对应。
  5. address_local_no 可为空;非空时必须为四位数字。

退出码非 0 即失败,可挂 pre-commit / CI。
用法: python3 citizenchain/scripts/check_code_immutable.py [--db <path>]
"""
import argparse
from pathlib import Path
import sqlite3
import sys

CHINA_DIR = Path(__file__).resolve().parent.parent / "onchina/src/cid/china"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--db", default=str(CHINA_DIR / "china.sqlite"))
    args = ap.parse_args()

    # 校验不修改源数据，错误路径不能生成空数据库。
    conn = sqlite3.connect(Path(args.db).resolve().as_uri() + "?mode=ro", uri=True)
    fail = []

    dup_towns = conn.execute(
        "SELECT province_code, city_code, code, COUNT(*) c FROM towns "
        "GROUP BY province_code, city_code, code HAVING c > 1"
    ).fetchall()
    if dup_towns:
        fail.append(f"towns 重复 (pc,cc,code) {len(dup_towns)} 组,例:{dup_towns[:5]}")

    dup_cities = conn.execute(
        "SELECT province_code, code, COUNT(*) c FROM cities "
        "GROUP BY province_code, code HAVING c > 1"
    ).fetchall()
    if dup_cities:
        fail.append(f"cities 重复 (pc,code) {len(dup_cities)} 组,例:{dup_cities[:5]}")

    dup_provinces = conn.execute(
        "SELECT code, COUNT(*) c FROM provinces GROUP BY code HAVING c > 1"
    ).fetchall()
    if dup_provinces:
        fail.append(f"provinces 重复 code {len(dup_provinces)} 组,例:{dup_provinces[:5]}")

    dup_province_names = conn.execute(
        "SELECT name, COUNT(*) c FROM provinces GROUP BY name HAVING c > 1"
    ).fetchall()
    if dup_province_names:
        fail.append(
            f"provinces 重复 name {len(dup_province_names)} 组,例:{dup_province_names[:5]}"
        )

    dup_city_names = conn.execute(
        "SELECT name, COUNT(*) c, GROUP_CONCAT(province_code || '/' || code) scopes "
        "FROM cities GROUP BY name HAVING c > 1"
    ).fetchall()
    if dup_city_names:
        fail.append(f"cities 全国重复 name {len(dup_city_names)} 组,例:{dup_city_names[:5]}")

    old_tables = {
        row[0]
        for row in conn.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall()
    }
    forbidden_tables = {
        "address_units",
        "province_tombstones",
        "city_tombstones",
        "town_tombstones",
        "admin_division_change_log",
        "admin_division_versions",
        "".join(("vill", "ages")),
    }
    stale_tables = sorted(old_tables.intersection(forbidden_tables))
    if stale_tables:
        fail.append(f"旧地址/墓碑/变更表必须清除:{stale_tables}")

    has_addresses = "addresses" in old_tables
    if not has_addresses:
        fail.append("addresses 表缺失:镇下完整地址数据不可用")
    else:
        address_columns = {
            row[1] for row in conn.execute("PRAGMA table_info(addresses)").fetchall()
        }
        stale_columns = sorted(
            address_columns.intersection({"address_unit_id", "raw_name", "source_code"})
        )
        if stale_columns:
            fail.append(f"addresses 仍含旧字段:{stale_columns}")

        bad_name_codes = conn.execute(
            "SELECT province_code, city_code, town_code, address_name_code "
            "FROM addresses "
            "WHERE length(address_name_code) <> 3 "
            "OR address_name_code NOT GLOB '[0-9][0-9][0-9]' "
            "OR address_name_code = '000' "
            "LIMIT 5"
        ).fetchall()
        if bad_name_codes:
            fail.append(f"address_name_code 必须为 001-999,例:{bad_name_codes[:5]}")

        bad_local_numbers = conn.execute(
            "SELECT province_code, city_code, town_code, address_name_code, address_local_no "
            "FROM addresses "
            "WHERE address_local_no <> '' "
            "AND (length(address_local_no) <> 4 "
            "OR address_local_no NOT GLOB '[0-9][0-9][0-9][0-9]' "
            "OR address_local_no = '0000') "
            "LIMIT 5"
        ).fetchall()
        if bad_local_numbers:
            fail.append(f"address_local_no 非空时必须为 0001-9999,例:{bad_local_numbers[:5]}")

        code_to_many_names = conn.execute(
            "SELECT province_code, city_code, town_code, address_name_code, COUNT(DISTINCT address_name) c "
            "FROM addresses "
            "GROUP BY province_code, city_code, town_code, address_name_code "
            "HAVING c > 1 LIMIT 5"
        ).fetchall()
        if code_to_many_names:
            fail.append(f"同镇 address_name_code 对应多个 address_name,例:{code_to_many_names[:5]}")

        name_to_many_codes = conn.execute(
            "SELECT province_code, city_code, town_code, address_name, COUNT(DISTINCT address_name_code) c "
            "FROM addresses "
            "GROUP BY province_code, city_code, town_code, address_name "
            "HAVING c > 1 LIMIT 5"
        ).fetchall()
        if name_to_many_codes:
            fail.append(f"同镇 address_name 对应多个 address_name_code,例:{name_to_many_codes[:5]}")

    conn.close()

    if fail:
        print("行政区 code 不可变校验 FAIL:", file=sys.stderr)
        for f in fail:
            print("  -", f, file=sys.stderr)
        return 1
    print("行政区 code 与 3+4 完整地址校验 PASS。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
