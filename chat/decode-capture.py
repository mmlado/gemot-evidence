#!/usr/bin/env python3
"""Decode what a non-member saw on a room's topic: `logosctl watch delivery_module` output and the group id."""
import base64, json, sys

def varint(b, i):
    r = s = 0
    while True:
        c = b[i]; i += 1; r |= (c & 0x7F) << s; s += 7
        if c < 0x80:
            return r, i

def main(path, gid):
    gid = gid.encode()
    print("time_ns | source | bytes | conversation_hint in clear | MLS epoch | MLS content type (1 app, 2 proposal, 3 commit)")
    for line in open(path):
        try:
            d = json.loads(line).get("data", {})
        except ValueError:
            continue
        if "arg2" not in d:
            continue
        raw = d["arg2"].get("_bytes", "")
        b = base64.urlsafe_b64decode(raw + "=" * (-len(raw) % 4))
        hint = None
        if b and b[0] == 0x0A:
            n, j = varint(b, 1)
            hint = b[j:j + n].decode(errors="replace")
        epoch = ctype = None
        k = b.find(b"\x00\x01\x00\x02" + bytes([len(gid)]) + gid)
        if k >= 0:
            p = k + 5 + len(gid)
            epoch, ctype = int.from_bytes(b[p:p + 8], "big"), b[p + 8]
        print(d.get("arg4"), "|", d.get("arg3"), "|", len(b), "|", hint, "|", epoch, "|", ctype)

if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit("usage: decode-capture.py <watch-output.jsonl> <group id>")
    main(sys.argv[1], sys.argv[2])
