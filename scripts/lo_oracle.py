#!/usr/bin/env python3
"""Secondary-oracle adjudication via LibreOffice UNO (§8.4).

Never `soffice --convert-to`: its OOXML recalc-on-load default is "never
recalculate" and it silently emits the file's cached values — a silently
passing oracle. This script loads each disputed workbook, calls
document.calculateAll() explicitly, reads the disputed cells, and rules:

  xlc_correct   — LO's recomputation agrees with XLC, not the cache
                  (stale/display-rounded cache: oracle noise, not a bug)
  xlc_wrong     — LO agrees with the cached value, not XLC (real bug)
  all_disagree  — three different answers (flagged for study)
  unreadable    — LO could not open/locate the cell

Run with LibreOffice's bundled python:
  lo-root/opt/libreoffice26.2/program/python scripts/lo_oracle.py \
      corpus/work/receipt7-failures.jsonl docs/benchmarks/oracle-adjudication.json
"""
import json
import os
import re
import subprocess
import sys
import time

LO_PROGRAM = os.path.expanduser("~/opt/lo-root/opt/libreoffice26.2/program")


def start_soffice():
    profile = "file:///tmp/lo-oracle-profile"
    proc = subprocess.Popen(
        [
            os.path.join(LO_PROGRAM, "soffice"),
            "--headless",
            "--norestore",
            "--nologo",
            f"-env:UserInstallation={profile}",
            "--accept=socket,host=localhost,port=2002;urp;",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return proc


def connect(retries=60):
    import uno

    ctx_local = uno.getComponentContext()
    resolver = ctx_local.ServiceManager.createInstanceWithContext(
        "com.sun.star.bridge.UnoUrlResolver", ctx_local
    )
    for _ in range(retries):
        try:
            ctx = resolver.resolve(
                "uno:socket,host=localhost,port=2002;urp;StarOffice.ComponentContext"
            )
            smgr = ctx.ServiceManager
            desktop = smgr.createInstanceWithContext(
                "com.sun.star.frame.Desktop", ctx
            )
            return desktop
        except Exception:
            time.sleep(1)
    raise RuntimeError("could not connect to soffice UNO socket")


def make_prop(name, value):
    from com.sun.star.beans import PropertyValue

    p = PropertyValue()
    p.Name = name
    p.Value = value
    return p


def parse_a1(cell):
    m = re.match(r"([A-Z]+)(\d+)", cell)
    col = 0
    for ch in m.group(1):
        col = col * 26 + ord(ch) - 64
    return col - 1, int(m.group(2)) - 1


def parse_num(s):
    try:
        return float(s)
    except (TypeError, ValueError):
        return None


def agrees(a, b):
    if a is None or b is None:
        return False
    if a == b:
        return True
    scale = max(abs(a), abs(b), 1e-300)
    return abs(a - b) / scale < 1e-9


def main():
    failures_path, out_path = sys.argv[1], sys.argv[2]
    by_file = {}
    for line in open(failures_path):
        d = json.loads(line)
        by_file.setdefault(d["file"], []).append(d)

    proc = start_soffice()
    try:
        desktop = connect()
        verdicts = {"xlc_correct": 0, "xlc_wrong": 0, "all_disagree": 0, "unreadable": 0}
        samples = []
        files_done = 0
        for path, cells in sorted(by_file.items()):
            url = "file://" + os.path.abspath(path)
            try:
                doc = desktop.loadComponentFromURL(
                    url, "_blank", 0, (make_prop("Hidden", True), make_prop("ReadOnly", True))
                )
                if doc is None:
                    verdicts["unreadable"] += len(cells)
                    continue
            except Exception:
                verdicts["unreadable"] += len(cells)
                continue
            try:
                # THE step --convert-to silently skips:
                doc.calculateAll()
                sheets = doc.getSheets()
                for d in cells:
                    try:
                        sheet = sheets.getByName(d["sheet"])
                        c, r = parse_a1(d["cell"])
                        cell = sheet.getCellByPosition(c, r)
                        lo_val = cell.getValue()  # numeric result
                        xlc = parse_num(d["got"])
                        cache = parse_num(d["expected"])
                        lo_x = agrees(lo_val, xlc)
                        lo_c = agrees(lo_val, cache)
                        if lo_x and not lo_c:
                            verdict = "xlc_correct"
                        elif lo_c and not lo_x:
                            verdict = "xlc_wrong"
                        elif lo_x and lo_c:
                            # Cache and XLC nearly equal; LO agrees with both
                            # (sub-tolerance dispute — counts for XLC).
                            verdict = "xlc_correct"
                        else:
                            verdict = "all_disagree"
                        verdicts[verdict] += 1
                        if len(samples) < 200:
                            samples.append(
                                {
                                    "file": os.path.basename(path),
                                    "sheet": d["sheet"],
                                    "cell": d["cell"],
                                    "formula": d["formula"][:120],
                                    "cache": d["expected"],
                                    "xlc": d["got"],
                                    "libreoffice": lo_val,
                                    "verdict": verdict,
                                }
                            )
                    except Exception:
                        verdicts["unreadable"] += 1
            finally:
                try:
                    doc.close(False)
                except Exception:
                    pass
            files_done += 1
            print(f"  {files_done}/{len(by_file)} {os.path.basename(path)}", flush=True)

        total = sum(verdicts.values())
        out = {
            "method": "LibreOffice 26.2.5 headless via UNO, explicit calculateAll() (never --convert-to, per XLC.md 8.4)",
            "disputed_cells": total,
            "verdicts": verdicts,
            "xlc_correct_pct": 100.0 * verdicts["xlc_correct"] / max(total, 1),
            "samples": samples,
        }
        json.dump(out, open(out_path, "w"), indent=1)
        print(json.dumps(verdicts, indent=1))
        print(f"-> {out_path}")
    finally:
        proc.terminate()


if __name__ == "__main__":
    main()
