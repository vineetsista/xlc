#!/usr/bin/env python3
"""Compact per-sample evidence dumps for the precision audit.

For each sample in docs/precision/<detector>.json, prints the deviant
formula, the exemplar, and (for range findings) the cached values of the
boundary cells the two ranges disagree about — the data that decides
whether the deviation is a defect or deliberate.

Usage: audit_dump.py <detector.json> [start] [count]
"""
import json
import re
import sys
import zipfile
from functools import lru_cache


@lru_cache(maxsize=64)
def sheet_xml(path, sheet_name):
    z = zipfile.ZipFile(path)
    wb = z.read('xl/workbook.xml').decode('utf8', 'replace')
    rels = z.read('xl/_rels/workbook.xml.rels').decode('utf8', 'replace')
    rid2t = dict(re.findall(r'Id="([^"]+)"[^>]*Target="([^"]+)"', rels))
    m = re.search(r'<sheet name="' + re.escape(sheet_name.replace('&', '&amp;')) + r'"[^>]*r:id="([^"]+)"', wb)
    if not m:
        return None, None
    target = rid2t.get(m.group(1), '')
    target = target if target.startswith('xl/') else 'xl/' + target
    try:
        x = z.read(target).decode('utf8', 'replace')
    except KeyError:
        return None, None
    shared = []
    if 'xl/sharedStrings.xml' in z.namelist():
        ss = z.read('xl/sharedStrings.xml').decode('utf8', 'replace')
        shared = [re.sub(r'<[^>]+>', '', si) for si in re.findall(r'<si>(.*?)</si>', ss, re.S)]
    return x, shared


def cell_value(x, shared, cell):
    m = re.search(r'<c r="' + cell + r'"(?:\s[^>]*)?(/>|>)', x)
    if not m or m.group(1) == '/>':
        return '·blank'
    end = x.find('</c>', m.end())
    frag = x[m.end():end]
    vm = re.search(r'<v>(.*?)</v>', frag)
    if not vm:
        return '·blank'
    attrs = x[m.start():m.end()]
    if 't="s"' in attrs:
        try:
            return repr(shared[int(vm.group(1))][:24])
        except (ValueError, IndexError):
            return '?s'
    return vm.group(1)[:20]


def a1(col, row):
    s = ''
    col += 1
    while col:
        col, r = divmod(col - 1, 26)
        s = chr(65 + r) + s
    return f"{s}{row + 1}"


def parse_a1(cell):
    m = re.match(r'([A-Z]+)(\d+)', cell)
    letters, digits = m.groups()
    col = 0
    for ch in letters:
        col = col * 26 + ord(ch) - 64
    return col - 1, int(digits) - 1


def main():
    path = sys.argv[1]
    start = int(sys.argv[2]) if len(sys.argv) > 2 else 0
    count = int(sys.argv[3]) if len(sys.argv) > 3 else 50
    doc = json.load(open(path))
    for i, s in enumerate(doc['samples'][start:start + count], start):
        print(f"--- [{i}] {s['file'].split('/')[-1]} {s['sheet']}!{s['cell']}")
        print(f"    proof: {s['proof'][:200]}")
        # For range findings, show boundary-cell values.
        ranges = re.findall(r'([A-Z]+\d+):([A-Z]+\d+)', s['formula'])
        x, shared = sheet_xml(s['file'], s['sheet'])
        if x and ranges:
            for (c1, c2) in ranges[:2]:
                col1, row1 = parse_a1(c1)
                col2, row2 = parse_a1(c2)
                # boundary neighborhood: one beyond each end
                probes = []
                if col1 == col2:  # vertical
                    for r in [row1 - 1, row1, row2, row2 + 1]:
                        if r >= 0:
                            probes.append(a1(col1, r))
                elif row1 == row2:  # horizontal
                    for c in [col1 - 1, col1, col2, col2 + 1]:
                        if c >= 0:
                            probes.append(a1(c, row1))
                vals = ', '.join(f"{p}={cell_value(x, shared, p)}" for p in probes)
                print(f"    range {c1}:{c2} boundary: {vals}")
        print()


if __name__ == '__main__':
    main()
