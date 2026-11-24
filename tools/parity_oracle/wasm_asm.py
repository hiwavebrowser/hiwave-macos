"""Tiny WebAssembly binary assembler for test modules (types, imports, funcs, exports, code)."""
def uleb(n):
    out = bytearray()
    while True:
        b = n & 0x7f; n >>= 7
        out.append(b | (0x80 if n else 0))
        if not n: return bytes(out)
def sleb(n):
    out = bytearray()
    while True:
        b = n & 0x7f; n >>= 7
        done = (n == 0 and not b & 0x40) or (n == -1 and b & 0x40)
        out.append(b | (0 if done else 0x80))
        if done: return bytes(out)
def vec(items): return uleb(len(items)) + b"".join(items)
def name(s): b = s.encode(); return uleb(len(b)) + b
def section(i, body): return bytes([i]) + uleb(len(body)) + body
I32, I64, F32, F64 = 0x7f, 0x7e, 0x7d, 0x7c
def module(types, imports, funcs, exports):
    """types: [(params, results)]; imports: [(mod, name, typeidx)]; funcs: [(typeidx, locals, code)];
    exports: [(name, funcidx)]."""
    t = vec([b"\x60" + vec([bytes([p]) for p in ps]) + vec([bytes([r]) for r in rs]) for ps, rs in types])
    out = b"\x00asm\x01\x00\x00\x00" + section(1, t)
    if imports:
        out += section(2, vec([name(m) + name(n) + b"\x00" + uleb(ti) for m, n, ti in imports]))
    out += section(3, vec([uleb(ti) for ti, _, _ in funcs]))
    out += section(7, vec([name(n) + b"\x00" + uleb(fi) for n, fi in exports]))
    bodies = []
    for _, locs, code in funcs:
        body = vec([uleb(c) + bytes([ty]) for c, ty in locs]) + code + b"\x0b"
        bodies.append(uleb(len(body)) + body)
    return out + section(10, vec(bodies))
# Re-entry: import env.cb (i32)->i32 = func 0; inner (i32)->i32 = x+1 (func 1); outer (i32)->i32 = cb(x) (func 2)
REENTRY = module([([I32], [I32])], [("env", "cb", 0)],
                 [(0, [], b"\x20\x00\x41\x01\x6a"), (0, [], b"\x20\x00\x10\x00")],
                 [("inner", 1), ("outer", 2)])
if __name__ == "__main__":
    print(list(REENTRY))

def module2(types, imports, funcs, exports, memory=None):
    """imports: [(mod, name, ("func", typeidx) | ("memory", min, max|None))];
    exports: [(name, kind, idx)] with kind 0 func / 2 memory; memory: (min, max|None) defined here."""
    t = vec([b"\x60" + vec([bytes([p]) for p in ps]) + vec([bytes([r]) for r in rs]) for ps, rs in types])
    out = b"\x00asm\x01\x00\x00\x00" + section(1, t)
    def limits(mn, mx): return (b"\x00" + uleb(mn)) if mx is None else (b"\x01" + uleb(mn) + uleb(mx))
    if imports:
        items = []
        for m, n, d in imports:
            if d[0] == "func": items.append(name(m) + name(n) + b"\x00" + uleb(d[1]))
            else: items.append(name(m) + name(n) + b"\x02" + limits(d[1], d[2]))
        out += section(2, vec(items))
    out += section(3, vec([uleb(ti) for ti, _, _ in funcs]))
    if memory: out += section(5, vec([limits(*memory)]))
    out += section(7, vec([name(n) + bytes([k]) + uleb(i) for n, k, i in exports]))
    bodies = []
    for _, locs, code in funcs:
        body = vec([uleb(c) + bytes([ty]) for c, ty in locs]) + code + b"\x0b"
        bodies.append(uleb(len(body)) + body)
    return out + section(10, vec(bodies))
# imported memory; load(p) = i32.load p; store(p, v) = i32.store p v; size() = memory.size; grow(n) = memory.grow n
MEMMOD = module2([([I32], [I32]), ([I32, I32], []), ([], [I32])], [("env", "memory", ("memory", 1, 2))],
                 [(0, [], b"\x20\x00\x28\x02\x00"), (1, [], b"\x20\x00\x20\x01\x36\x02\x00"), (2, [], b"\x3f\x00"), (0, [], b"\x20\x00\x40\x00")],
                 [("load", 0, 0), ("store", 0, 1), ("size", 0, 2), ("grow", 0, 3)])
# module-defined memory (1 page), exported as "mem"
OWNMEM = module2([([], [I32])], [], [(0, [], b"\x3f\x00")], [("size", 0, 0), ("mem", 2, 0)], memory=(1, None))

def module3(types, imports, funcs, exports, elems=()):
    """imports: [(mod, name, desc)] with desc ("func", ti) | ("table", min, max|None) |
    ("global", valtype, mutable); exports: [(name, kind, idx)]; elems: [(offset, [funcidx...])] into table 0."""
    t = vec([b"\x60" + vec([bytes([p]) for p in ps]) + vec([bytes([r]) for r in rs]) for ps, rs in types])
    out = b"\x00asm\x01\x00\x00\x00" + section(1, t)
    def limits(mn, mx): return (b"\x00" + uleb(mn)) if mx is None else (b"\x01" + uleb(mn) + uleb(mx))
    items = []
    for m, n, d in imports:
        if d[0] == "func": items.append(name(m) + name(n) + b"\x00" + uleb(d[1]))
        elif d[0] == "table": items.append(name(m) + name(n) + b"\x01\x70" + limits(d[1], d[2]))
        else: items.append(name(m) + name(n) + b"\x03" + bytes([d[1], 1 if d[2] else 0]))
    if items: out += section(2, vec(items))
    out += section(3, vec([uleb(ti) for ti, _, _ in funcs]))
    out += section(7, vec([name(n) + bytes([k]) + uleb(i) for n, k, i in exports]))
    if elems:
        out += section(9, vec([b"\x00\x41" + sleb(off) + b"\x0b" + vec([uleb(f) for f in fs]) for off, fs in elems]))
    bodies = []
    for _, locs, code in funcs:
        body = vec([uleb(c) + bytes([ty]) for c, ty in locs]) + code + b"\x0b"
        bodies.append(uleb(len(body)) + body)
    return out + section(10, vec(bodies))
# imports: table env.table (2, 4) funcref; global env.g i32 mutable; global env.nan f64 immutable
# funcs (no func imports, so func 0 is the first defined):
#   0 seven() -> i32 = 7                         type 0 ([], [I32])
#   1 dispatch(i) -> i32 = call_indirect type 0 (i) table 0     type 1 ([I32],[I32])
#   2 getg() -> i32 = global.get 0             type 0
#   3 nan() -> f64 = global.get 1               type 2 ([], [F64])
# elem: table[1] = func 0
TABMOD = module3([([], [I32]), ([I32], [I32]), ([], [F64])],
                 [("env", "table", ("table", 2, 4)), ("env", "g", ("global", I32, True)), ("env", "nan", ("global", F64, False))],
                 [(0, [], b"\x41\x07"), (1, [], b"\x20\x00\x11\x00\x00"), (0, [], b"\x23\x00"), (2, [], b"\x23\x01")],
                 [("seven", 0, 0), ("dispatch", 0, 1), ("getg", 0, 2), ("nan", 0, 3)],
                 elems=[(1, [0])])
